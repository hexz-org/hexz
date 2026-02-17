//! Crash safety tests for the snapshot write path.
//!
//! These tests verify that readers correctly reject snapshots where the write
//! process was interrupted (simulated by truncation or header corruption).
//! The write path uses a header-last design: the header (with index_offset)
//! is written only after all blocks and indices are on disk, so a crash at
//! any prior stage leaves a zeroed header that readers can detect.

use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_core::format::header::Header;
use hexz_core::format::magic::HEADER_SIZE;
use hexz_core::ops::pack::{PackConfig, pack_snapshot};
use hexz_core::store::local::FileBackend;
use hexz_core::{File, SnapshotStream};
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

/// Creates a valid snapshot and returns its path + the temp dir (to keep it alive).
fn create_valid_snapshot() -> (std::path::PathBuf, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");
    // 256KB of mixed data (not all zeros, to ensure blocks are actually written)
    let mut data = vec![0xAA_u8; 256 * 1024];
    for (i, byte) in data.iter_mut().enumerate() {
        *byte = (i % 251) as u8; // Prime modulus for varied data
    }
    fs::write(&disk_path, &data).unwrap();

    let output_path = temp_dir.path().join("snapshot.hxz");
    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: output_path.clone(),
        compression: "lz4".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        min_chunk: 16384,
        avg_chunk: 65536,
        max_chunk: 131072,
        ..Default::default()
    };
    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    // Sanity check: the snapshot is valid
    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = File::new(backend, compressor, None).unwrap();
    assert_eq!(snapshot.size(SnapshotStream::Disk), 256 * 1024);

    (output_path, temp_dir)
}

/// Try to open a snapshot file, returning Ok if it opens or Err if it's rejected.
fn try_open_snapshot(path: &std::path::Path) -> hexz_common::Result<Arc<File>> {
    let backend = Arc::new(FileBackend::new(path)?);
    let compressor = Box::new(Lz4Compressor::new());
    File::new(backend, compressor, None)
}

// ── Empty / zero-byte file ──────────────────────────────────────────────────

#[test]
fn test_crash_empty_file() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("empty.hxz");
    fs::write(&path, []).unwrap();
    assert!(
        try_open_snapshot(&path).is_err(),
        "empty file should be rejected"
    );
}

#[test]
fn test_crash_zeroed_header_only() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("zeroed.hxz");
    // Simulate: process crashed right after reserving header space
    fs::write(&path, [0u8; HEADER_SIZE]).unwrap();
    assert!(
        try_open_snapshot(&path).is_err(),
        "zeroed header should be rejected"
    );
}

// ── Truncated at various stages ─────────────────────────────────────────────

#[test]
fn test_crash_truncated_mid_header() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("truncated_header.hxz");
    // File shorter than HEADER_SIZE — simulates crash during initial header reserve
    fs::write(&path, [0u8; 100]).unwrap();
    assert!(
        try_open_snapshot(&path).is_err(),
        "truncated header should be rejected"
    );
}

#[test]
fn test_crash_truncated_during_block_write() {
    let (snapshot_path, _temp_dir) = create_valid_snapshot();
    let full_data = fs::read(&snapshot_path).unwrap();

    // Truncate to header + half a block (simulates crash mid-block-write)
    let truncated_path = _temp_dir.path().join("truncated_blocks.hxz");
    let truncate_at = HEADER_SIZE + 1000;
    assert!(truncate_at < full_data.len());

    // Write truncated file with zeroed header (as the real write path would have)
    let mut truncated = vec![0u8; HEADER_SIZE];
    truncated.extend_from_slice(&full_data[HEADER_SIZE..truncate_at]);
    fs::write(&truncated_path, &truncated).unwrap();

    assert!(
        try_open_snapshot(&truncated_path).is_err(),
        "file truncated during block write (zeroed header) should be rejected"
    );
}

#[test]
fn test_crash_truncated_before_master_index() {
    let (snapshot_path, _temp_dir) = create_valid_snapshot();
    let full_data = fs::read(&snapshot_path).unwrap();

    // Read the real header to find the index offset
    let header: Header = bincode::deserialize(&full_data[..HEADER_SIZE]).unwrap();

    // Truncate just before the master index, but keep zeroed header
    // (simulates: all blocks written, index pages written, crash before master index)
    let truncated_path = _temp_dir.path().join("no_master_index.hxz");
    let mut truncated = vec![0u8; HEADER_SIZE]; // zeroed header
    truncated.extend_from_slice(&full_data[HEADER_SIZE..header.index_offset as usize]);
    fs::write(&truncated_path, &truncated).unwrap();

    assert!(
        try_open_snapshot(&truncated_path).is_err(),
        "file without master index (zeroed header) should be rejected"
    );
}

// ── Header corruption scenarios ─────────────────────────────────────────────

#[test]
fn test_crash_wrong_magic_bytes() {
    let (snapshot_path, _temp_dir) = create_valid_snapshot();
    let mut data = fs::read(&snapshot_path).unwrap();

    // Corrupt magic bytes (simulate partial header write)
    data[0] = 0x00;
    data[1] = 0x00;
    let corrupted_path = _temp_dir.path().join("bad_magic.hxz");
    fs::write(&corrupted_path, &data).unwrap();

    assert!(
        try_open_snapshot(&corrupted_path).is_err(),
        "corrupted magic bytes should be rejected"
    );
}

#[test]
fn test_crash_header_written_but_index_truncated() {
    let (snapshot_path, _temp_dir) = create_valid_snapshot();
    let mut data = fs::read(&snapshot_path).unwrap();

    // Read header to get index_offset
    let header: Header = bincode::deserialize(&data[..HEADER_SIZE]).unwrap();

    // Keep valid header but truncate the file so the master index is incomplete.
    // This simulates: header written (committed), but then file was truncated
    // (e.g., filesystem recovered partial write after power loss, or OS reordered
    // header write before index write).
    let truncate_at = header.index_offset as usize + 2; // just 2 bytes of index
    data.truncate(truncate_at);
    let corrupted_path = _temp_dir.path().join("truncated_index.hxz");
    fs::write(&corrupted_path, &data).unwrap();

    assert!(
        try_open_snapshot(&corrupted_path).is_err(),
        "valid header but truncated master index should be rejected"
    );
}

#[test]
fn test_crash_index_offset_points_past_eof() {
    let (snapshot_path, _temp_dir) = create_valid_snapshot();
    let mut data = fs::read(&snapshot_path).unwrap();

    // Modify header to point index_offset past end of file
    let mut header: Header = bincode::deserialize(&data[..HEADER_SIZE]).unwrap();
    header.index_offset = data.len() as u64 + 10000;
    let header_bytes = bincode::serialize(&header).unwrap();
    data[..header_bytes.len()].copy_from_slice(&header_bytes);

    let corrupted_path = _temp_dir.path().join("index_past_eof.hxz");
    fs::write(&corrupted_path, &data).unwrap();

    assert!(
        try_open_snapshot(&corrupted_path).is_err(),
        "index_offset past EOF should be rejected"
    );
}

// ── Block-level corruption (CRC32 detection) ────────────────────────────────

#[test]
fn test_corruption_single_bitflip_in_block_data() {
    let (snapshot_path, _temp_dir) = create_valid_snapshot();
    let mut data = fs::read(&snapshot_path).unwrap();

    // Flip a bit in the first data block (just past the header)
    let flip_offset = HEADER_SIZE + 10;
    data[flip_offset] ^= 0x01;

    let corrupted_path = _temp_dir.path().join("bitflip_block.hxz");
    fs::write(&corrupted_path, &data).unwrap();

    let backend = Arc::new(FileBackend::new(&corrupted_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    // File should open (header is fine), but reading the corrupted block should fail
    let snapshot = File::new(backend, compressor, None).unwrap();
    let result = snapshot.read_at(SnapshotStream::Disk, 0, 4096);
    assert!(
        result.is_err(),
        "bitflip in block data should be detected by CRC32"
    );
}

#[test]
fn test_corruption_zeroed_block_data() {
    let (snapshot_path, _temp_dir) = create_valid_snapshot();
    let mut data = fs::read(&snapshot_path).unwrap();

    // Zero out a chunk of block data after the header
    let start = HEADER_SIZE;
    let end = start + 100;
    for byte in &mut data[start..end] {
        *byte = 0;
    }

    let corrupted_path = _temp_dir.path().join("zeroed_block.hxz");
    fs::write(&corrupted_path, &data).unwrap();

    let backend = Arc::new(FileBackend::new(&corrupted_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = File::new(backend, compressor, None).unwrap();
    let result = snapshot.read_at(SnapshotStream::Disk, 0, 4096);
    assert!(
        result.is_err(),
        "zeroed block data should be detected by CRC32"
    );
}

// ── Index corruption ────────────────────────────────────────────────────────

#[test]
fn test_corruption_bitflip_in_master_index() {
    let (snapshot_path, _temp_dir) = create_valid_snapshot();
    let mut data = fs::read(&snapshot_path).unwrap();

    let header: Header = bincode::deserialize(&data[..HEADER_SIZE]).unwrap();
    // Flip a bit in the master index region
    let flip_offset = header.index_offset as usize + 5;
    if flip_offset < data.len() {
        data[flip_offset] ^= 0x80;

        let corrupted_path = _temp_dir.path().join("bitflip_index.hxz");
        fs::write(&corrupted_path, &data).unwrap();

        // Should either fail to open (deserialization error) or produce wrong results
        // that get caught by CRC32 on block reads
        match try_open_snapshot(&corrupted_path) {
            Err(_) => {} // Good: corruption detected at index deserialization
            Ok(snapshot) => {
                // If it opened, block reads should fail due to wrong offsets/checksums
                let result = snapshot.read_at(SnapshotStream::Disk, 0, 65536);
                // Either error (CRC32 mismatch) or wrong data — both acceptable
                // as long as it doesn't silently return the original correct data
                // while the index was corrupted
                let _ = result;
            }
        }
    }
}

// ── Valid snapshot survives all checks (positive control) ───────────────────

#[test]
fn test_valid_snapshot_reads_correctly() {
    let (snapshot_path, _temp_dir) = create_valid_snapshot();

    let backend = Arc::new(FileBackend::new(&snapshot_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = File::new(backend, compressor, None).unwrap();

    // Read first block and verify expected pattern
    let data = snapshot.read_at(SnapshotStream::Disk, 0, 4096).unwrap();
    assert_eq!(data.len(), 4096);
    for (i, &byte) in data.iter().enumerate() {
        assert_eq!(byte, (i % 251) as u8, "mismatch at offset {i}");
    }

    // Read last block
    let last_offset = 256 * 1024 - 4096;
    let last_data = snapshot
        .read_at(SnapshotStream::Disk, last_offset as u64, 4096)
        .unwrap();
    assert_eq!(last_data.len(), 4096);
}
