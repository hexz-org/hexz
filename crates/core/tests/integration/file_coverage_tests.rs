//! Tests targeting uncovered code paths in file.rs:
//! - CRC32 corruption detection
//! - Thin snapshot parent chain fallback (BLOCK_OFFSET_PARENT)
//! - Zero-length/sparse block handling
//! - Encrypted snapshot read path

use super::common;
use common::*;

use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_core::format::header::Header;
use hexz_core::format::index::MasterIndex;
use hexz_core::format::magic::HEADER_SIZE;
use hexz_core::ops::pack::{PackConfig, pack_snapshot};
use hexz_core::store::StorageBackend;
use hexz_core::store::local::FileBackend;
use hexz_core::{File, SnapshotStream};
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::Arc;
use tempfile::TempDir;

// ═══════════════════════════════════════════════════════════════════════════════
//  CRC32 Corruption Detection Tests
// ═══════════════════════════════════════════════════════════════════════════════

/// Helper to corrupt a byte at a given file offset.
fn corrupt_byte_at(path: &std::path::Path, offset: u64) {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    file.seek(SeekFrom::Start(offset)).unwrap();
    let mut byte = [0u8; 1];
    file.read_exact(&mut byte).unwrap();
    byte[0] ^= 0xFF; // Flip all bits
    file.seek(SeekFrom::Start(offset)).unwrap();
    file.write_all(&byte).unwrap();
    file.flush().unwrap();
}

/// Test that corrupting compressed block data triggers CRC32 error.
#[test]
fn test_crc32_corruption_detected() {
    let temp_dir = TempDir::new().unwrap();

    // Create snapshot with non-zero data (so blocks have real checksums)
    let disk_path = temp_dir.path().join("disk.img");
    let data = vec![0x42u8; 128 * 1024]; // Non-zero data
    fs::write(&disk_path, &data).unwrap();

    let snap_path = temp_dir.path().join("test.hxz");
    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: snap_path.clone(),
        compression: "lz4".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };
    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    // Read the header and index to find block offsets
    let backend = Arc::new(FileBackend::new(&snap_path).unwrap());
    let header_bytes = backend.read_exact(0, HEADER_SIZE).unwrap();
    let header: Header = bincode::deserialize(&header_bytes).unwrap();

    let index_bytes = backend
        .read_exact(
            header.index_offset,
            (backend.len() - header.index_offset) as usize,
        )
        .unwrap();
    let master: MasterIndex = bincode::deserialize(&index_bytes).unwrap();

    // Find the first non-zero block (has actual compressed data and checksum)
    let mut block_offset = None;
    for page_entry in &master.primary_pages {
        let page_bytes = backend
            .read_exact(page_entry.offset, page_entry.length as usize)
            .unwrap();
        let page: hexz_core::format::index::IndexPage = bincode::deserialize(&page_bytes).unwrap();
        for block in &page.blocks {
            if block.length > 0 && block.checksum != 0 {
                // Found a block with data and checksum
                block_offset = Some(block.offset);
                break;
            }
        }
        if block_offset.is_some() {
            break;
        }
    }

    let block_off = block_offset.expect("Should find at least one data block");

    // Corrupt a byte in the middle of the compressed block
    corrupt_byte_at(&snap_path, block_off + 5);

    // Now try to read — should detect corruption
    let backend2 = Arc::new(FileBackend::new(&snap_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = File::new(backend2, compressor, None).unwrap();

    let result = snapshot.read_at(SnapshotStream::Primary, 0, 65536);
    assert!(result.is_err(), "Should detect CRC32 corruption");

    // Verify it's specifically a corruption error
    if let Err(e) = result {
        let err_str = format!("{}", e);
        assert!(
            err_str.contains("Corrupt")
                || err_str.contains("corrupt")
                || err_str.contains("CRC")
                || err_str.contains("checksum")
                || err_str.contains("Checksum"),
            "Error should indicate corruption, got: {}",
            err_str
        );
    }
}

/// Test that valid data passes CRC32 check.
#[test]
fn test_crc32_valid_data_passes() {
    let temp_dir = TempDir::new().unwrap();

    let disk_path = temp_dir.path().join("disk.img");
    let data = vec![0x42u8; 128 * 1024];
    fs::write(&disk_path, &data).unwrap();

    let snap_path = temp_dir.path().join("test.hxz");
    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: snap_path.clone(),
        compression: "lz4".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };
    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    // Read without corruption — should succeed
    let backend = Arc::new(FileBackend::new(&snap_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = File::new(backend, compressor, None).unwrap();

    let read_data = snapshot.read_at(SnapshotStream::Primary, 0, 65536).unwrap();
    assert_eq!(read_data.len(), 65536);
    assert!(read_data.iter().all(|&b| b == 0x42));
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Sparse/Zero Block Tests
// ═══════════════════════════════════════════════════════════════════════════════

/// Test that all-zero blocks are stored sparsely and read back as zeros.
#[test]
fn test_zero_block_sparse_handling() {
    let temp_dir = TempDir::new().unwrap();

    // Create a disk image that is entirely zeros
    let disk_path = temp_dir.path().join("zeros.img");
    let data = vec![0u8; 256 * 1024];
    fs::write(&disk_path, &data).unwrap();

    let snap_path = temp_dir.path().join("zeros.hxz");
    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: snap_path.clone(),
        compression: "lz4".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };
    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    let backend = Arc::new(FileBackend::new(&snap_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = File::new(backend, compressor, None).unwrap();

    // Read all blocks — should be all zeros
    let read_data = snapshot
        .read_at(SnapshotStream::Primary, 0, 256 * 1024)
        .unwrap();
    assert_eq!(read_data.len(), 256 * 1024);
    assert!(
        read_data.iter().all(|&b| b == 0),
        "All-zero disk should read as zeros"
    );
}

/// Test mixed zero and non-zero blocks.
#[test]
fn test_mixed_zero_nonzero_blocks() {
    let temp_dir = TempDir::new().unwrap();

    let disk_path = temp_dir.path().join("mixed.img");

    // Block 0: non-zero, Block 1: all zeros, Block 2: non-zero, Block 3: all zeros
    let mut data = Vec::new();
    data.extend_from_slice(&vec![0xAA; 65536]); // Block 0
    data.extend_from_slice(&vec![0x00; 65536]); // Block 1 (sparse)
    data.extend_from_slice(&vec![0xBB; 65536]); // Block 2
    data.extend_from_slice(&vec![0x00; 65536]); // Block 3 (sparse)
    fs::write(&disk_path, &data).unwrap();

    let snap_path = temp_dir.path().join("mixed.hxz");
    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: snap_path.clone(),
        compression: "lz4".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };
    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    let backend = Arc::new(FileBackend::new(&snap_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = File::new(backend, compressor, None).unwrap();

    // Read each block and verify
    let block0 = snapshot.read_at(SnapshotStream::Primary, 0, 65536).unwrap();
    assert!(block0.iter().all(|&b| b == 0xAA));

    let block1 = snapshot
        .read_at(SnapshotStream::Primary, 65536, 65536)
        .unwrap();
    assert!(
        block1.iter().all(|&b| b == 0x00),
        "Sparse block should be zero"
    );

    let block2 = snapshot
        .read_at(SnapshotStream::Primary, 65536 * 2, 65536)
        .unwrap();
    assert!(block2.iter().all(|&b| b == 0xBB));

    let block3 = snapshot
        .read_at(SnapshotStream::Primary, 65536 * 3, 65536)
        .unwrap();
    assert!(
        block3.iter().all(|&b| b == 0x00),
        "Sparse block should be zero"
    );

    // Read spanning zero and non-zero block boundary
    let crossing = snapshot
        .read_at(SnapshotStream::Primary, 65536 - 100, 200)
        .unwrap();
    assert_eq!(crossing.len(), 200);
    // First 100 bytes from block 0 (0xAA), next 100 from block 1 (0x00)
    assert!(crossing[..100].iter().all(|&b| b == 0xAA));
    assert!(crossing[100..].iter().all(|&b| b == 0x00));
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Read at Into with Various Buffer Patterns
// ═══════════════════════════════════════════════════════════════════════════════

/// Test read_at_into when the read spans past the last page.
#[test]
fn test_read_past_last_page_zeroes() {
    let temp_dir = TempDir::new().unwrap();

    let disk_path = temp_dir.path().join("small.img");
    let data = vec![0x55u8; 65536]; // Exactly one block
    fs::write(&disk_path, &data).unwrap();

    let snap_path = temp_dir.path().join("small.hxz");
    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: snap_path.clone(),
        compression: "lz4".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };
    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    let backend = Arc::new(FileBackend::new(&snap_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = File::new(backend, compressor, None).unwrap();

    // Request more data than exists — excess should be zero-filled
    let mut buffer = vec![0xFF; 65536 + 1000];
    snapshot
        .read_at_into(SnapshotStream::Primary, 0, &mut buffer)
        .unwrap();

    // First block should be data, rest should be zeros
    assert!(buffer[..65536].iter().all(|&b| b == 0x55));
    assert!(buffer[65536..].iter().all(|&b| b == 0x00));
}

/// Test reading from secondary stream when there's no memory (size=0).
#[test]
fn test_read_empty_memory_stream() {
    let (snap_path, _) = create_simple_snapshot().unwrap();

    let backend = Arc::new(FileBackend::new(&snap_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = File::new(backend, compressor, None).unwrap();

    assert_eq!(snapshot.size(SnapshotStream::Secondary), 0);

    // Reading from empty secondary stream should return empty
    let data = snapshot.read_at(SnapshotStream::Secondary, 0, 100).unwrap();
    assert!(data.is_empty());

    // read_at_into should zero-fill the buffer
    let mut buffer = vec![0xFF; 100];
    snapshot
        .read_at_into(SnapshotStream::Secondary, 0, &mut buffer)
        .unwrap();
    assert!(buffer.iter().all(|&b| b == 0x00));
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Cross-Block Boundary Read Tests
// ═══════════════════════════════════════════════════════════════════════════════

/// Test reading across multiple block boundaries in a single call.
#[test]
fn test_cross_boundary_reads() {
    let temp_dir = TempDir::new().unwrap();

    let disk_path = temp_dir.path().join("multi.img");
    // 4 blocks of 65536 each, each filled with a different byte
    let mut data = Vec::new();
    for i in 0u8..4 {
        data.extend_from_slice(&vec![0x10 + i; 65536]);
    }
    fs::write(&disk_path, &data).unwrap();

    let snap_path = temp_dir.path().join("multi.hxz");
    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: snap_path.clone(),
        compression: "lz4".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };
    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    let backend = Arc::new(FileBackend::new(&snap_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = File::new(backend, compressor, None).unwrap();

    // Read spanning blocks 1 and 2 (middle of block 1 to middle of block 2)
    let mid_read = snapshot
        .read_at(SnapshotStream::Primary, 65536 + 32768, 65536)
        .unwrap();
    assert_eq!(mid_read.len(), 65536);
    // First half from block 1 (0x11)
    assert!(mid_read[..32768].iter().all(|&b| b == 0x11));
    // Second half from block 2 (0x12)
    assert!(mid_read[32768..].iter().all(|&b| b == 0x12));

    // Read spanning all 4 blocks
    let full = snapshot
        .read_at(SnapshotStream::Primary, 0, 4 * 65536)
        .unwrap();
    assert_eq!(full.len(), 4 * 65536);
    for i in 0u8..4 {
        let start = i as usize * 65536;
        let expected = 0x10 + i;
        assert!(
            full[start..start + 65536].iter().all(|&b| b == expected),
            "Block {} should be 0x{:02X}",
            i,
            expected
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Encrypted Read Path Tests
// ═══════════════════════════════════════════════════════════════════════════════

/// Test that encrypted + compressed data round-trips through decompress_and_verify.
#[test]
fn test_encrypted_read_with_crc_check() {
    use hexz_core::algo::encryption::aes_gcm::AesGcmEncryptor;

    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");
    let original_data: Vec<u8> = (0..128 * 1024).map(|i| (i % 256) as u8).collect();
    fs::write(&disk_path, &original_data).unwrap();

    let snap_path = temp_dir.path().join("encrypted.hxz");
    let password = "test_crc_encrypted";

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: snap_path.clone(),
        compression: "lz4".to_string(),
        encrypt: true,
        password: Some(password.to_string()),
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };
    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    // Open with correct password
    let backend = Arc::new(FileBackend::new(&snap_path).unwrap());
    let header_bytes = backend.read_exact(0, HEADER_SIZE).unwrap();
    let header: Header = bincode::deserialize(&header_bytes).unwrap();

    let compressor = Box::new(Lz4Compressor::new());
    let encryptor = header.encryption.as_ref().map(|params| {
        Box::new(
            AesGcmEncryptor::new(password.as_bytes(), &params.salt, params.iterations).unwrap(),
        ) as Box<dyn hexz_core::algo::encryption::Encryptor>
    });

    let snapshot = File::new(backend, compressor, encryptor).unwrap();

    // Read and verify data round-trips correctly through decrypt+decompress+CRC path
    let read_data = snapshot
        .read_at(SnapshotStream::Primary, 0, 128 * 1024)
        .unwrap();
    assert_eq!(read_data.len(), 128 * 1024);
    assert_eq!(&read_data[..], &original_data[..]);
}

/// Test corrupted encrypted snapshot triggers error.
#[test]
fn test_encrypted_corruption_detected() {
    use hexz_core::algo::encryption::aes_gcm::AesGcmEncryptor;

    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");
    let data = vec![0x42u8; 128 * 1024];
    fs::write(&disk_path, &data).unwrap();

    let snap_path = temp_dir.path().join("encrypted.hxz");
    let password = "corrupt_test";

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: snap_path.clone(),
        compression: "lz4".to_string(),
        encrypt: true,
        password: Some(password.to_string()),
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };
    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    // Find a data block and corrupt it
    let backend = Arc::new(FileBackend::new(&snap_path).unwrap());
    let header_bytes = backend.read_exact(0, HEADER_SIZE).unwrap();
    let header: Header = bincode::deserialize(&header_bytes).unwrap();

    let index_bytes = backend
        .read_exact(
            header.index_offset,
            (backend.len() - header.index_offset) as usize,
        )
        .unwrap();
    let master: MasterIndex = bincode::deserialize(&index_bytes).unwrap();

    let mut block_off = None;
    for page_entry in &master.primary_pages {
        let page_bytes = backend
            .read_exact(page_entry.offset, page_entry.length as usize)
            .unwrap();
        let page: hexz_core::format::index::IndexPage = bincode::deserialize(&page_bytes).unwrap();
        for block in &page.blocks {
            if block.length > 0 && block.checksum != 0 {
                block_off = Some(block.offset);
                break;
            }
        }
        if block_off.is_some() {
            break;
        }
    }

    let offset = block_off.expect("Should find data block");
    corrupt_byte_at(&snap_path, offset + 10);

    // Try to read with the correct password — CRC should fail before decryption
    let backend2 = Arc::new(FileBackend::new(&snap_path).unwrap());
    let header_bytes2 = backend2.read_exact(0, HEADER_SIZE).unwrap();
    let header2: Header = bincode::deserialize(&header_bytes2).unwrap();

    let compressor = Box::new(Lz4Compressor::new());
    let encryptor = header2.encryption.as_ref().map(|params| {
        Box::new(
            AesGcmEncryptor::new(password.as_bytes(), &params.salt, params.iterations).unwrap(),
        ) as Box<dyn hexz_core::algo::encryption::Encryptor>
    });

    let snapshot = File::new(backend2, compressor, encryptor).unwrap();

    let result = snapshot.read_at(SnapshotStream::Primary, 0, 65536);
    assert!(
        result.is_err(),
        "Corrupted encrypted block should fail CRC or decryption"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Prefetcher and Cache Interaction Tests
// ═══════════════════════════════════════════════════════════════════════════════

/// Test that sequential reads benefit from prefetching (no errors).
#[test]
fn test_sequential_reads_with_prefetch() {
    let (snap_path, original_data) = create_simple_snapshot().unwrap();

    let backend = Arc::new(FileBackend::new(&snap_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = File::with_cache(
        backend,
        compressor,
        None,
        None,
        Some(4), // Prefetch 4 blocks
    )
    .unwrap();

    // Do sequential reads that should trigger prefetching
    let mut offset = 0u64;
    while offset < original_data.len() as u64 {
        let to_read = std::cmp::min(65536, original_data.len() - offset as usize);
        let data = snapshot
            .read_at(SnapshotStream::Primary, offset, to_read)
            .unwrap();
        assert_eq!(
            &data[..],
            &original_data[offset as usize..offset as usize + to_read]
        );
        offset += to_read as u64;
    }
}

/// Test that cache works correctly across repeated reads of same offset.
#[test]
fn test_repeated_reads_use_cache() {
    let (snap_path, original_data) = create_simple_snapshot().unwrap();

    let backend = Arc::new(FileBackend::new(&snap_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = File::with_cache(
        backend,
        compressor,
        None,
        Some(1024 * 1024), // 1MB cache
        None,
    )
    .unwrap();

    // Read same offset multiple times
    for _ in 0..10 {
        let data = snapshot.read_at(SnapshotStream::Primary, 0, 65536).unwrap();
        assert_eq!(&data[..], &original_data[..65536]);
    }
}
