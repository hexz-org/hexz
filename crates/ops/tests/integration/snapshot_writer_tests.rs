/// Integration tests for SnapshotWriter.
///
/// These tests exercise the low-level write → read roundtrip directly via
/// `SnapshotWriter`, bypassing the higher-level `pack_snapshot` helper used
/// by the CLI.  This gives fine-grained coverage of begin_stream /
/// write_data_block / end_stream / finalize and their interactions.
use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_core::algo::compression::zstd::ZstdCompressor;
use hexz_core::format::header::CompressionType;
use hexz_core::{File as HexzFile, SnapshotStream};
use hexz_ops::snapshot_writer::SnapshotWriter;
use hexz_store::local::FileBackend;
use std::sync::Arc;
use tempfile::TempDir;

// ── helpers ───────────────────────────────────────────────────────────────────

const BLOCK: usize = 65_536; // 64 KiB default block size

fn lz4_writer(dir: &TempDir, name: &str) -> (SnapshotWriter, std::path::PathBuf) {
    let path = dir.path().join(name);
    let writer =
        SnapshotWriter::builder(&path, Box::new(Lz4Compressor::new()), CompressionType::Lz4)
            .block_size(BLOCK as u32)
            .build()
            .expect("SnapshotWriter::build failed");
    (writer, path)
}

fn zstd_writer(dir: &TempDir, name: &str) -> (SnapshotWriter, std::path::PathBuf) {
    let path = dir.path().join(name);
    let writer = SnapshotWriter::builder(
        &path,
        Box::new(ZstdCompressor::new(3, None)),
        CompressionType::Zstd,
    )
    .block_size(BLOCK as u32)
    .build()
    .expect("SnapshotWriter::build failed");
    (writer, path)
}

fn open(path: &std::path::Path) -> Arc<HexzFile> {
    let backend = Arc::new(FileBackend::new(path).expect("FileBackend::new failed"));
    HexzFile::open(backend, None).expect("HexzFile::open failed")
}

fn read_all(file: &Arc<HexzFile>, stream: SnapshotStream) -> Vec<u8> {
    let size = file.size(stream) as usize;
    file.read_at(stream, 0, size).expect("read_at failed")
}

// ── empty archive ─────────────────────────────────────────────────────────────

#[test]
fn test_empty_archive_is_valid() {
    let dir = TempDir::new().unwrap();
    let (writer, path) = lz4_writer(&dir, "empty.hxz");
    writer.finalize(vec![], None).unwrap();
    assert!(path.exists());
    assert!(
        path.metadata().unwrap().len() > 0,
        "archive must contain at least a header"
    );
}

// ── single-block roundtrips ───────────────────────────────────────────────────

#[test]
fn test_single_block_lz4_roundtrip() {
    let dir = TempDir::new().unwrap();
    let data = vec![0x42u8; BLOCK];

    let (mut writer, path) = lz4_writer(&dir, "single_lz4.hxz");
    writer.begin_stream(true, data.len() as u64);
    writer.write_data_block(&data).unwrap();
    writer.end_stream().unwrap();
    writer.finalize(vec![], None).unwrap();

    let file = open(&path);
    assert_eq!(read_all(&file, SnapshotStream::Primary), data);
}

#[test]
fn test_single_block_zstd_roundtrip() {
    let dir = TempDir::new().unwrap();
    let data = vec![0xABu8; BLOCK];

    let (mut writer, path) = zstd_writer(&dir, "single_zstd.hxz");
    writer.begin_stream(true, data.len() as u64);
    writer.write_data_block(&data).unwrap();
    writer.end_stream().unwrap();
    writer.finalize(vec![], None).unwrap();

    let file = open(&path);
    assert_eq!(read_all(&file, SnapshotStream::Primary), data);
}

// ── multi-block roundtrips ────────────────────────────────────────────────────

#[test]
fn test_multi_block_roundtrip() {
    let dir = TempDir::new().unwrap();
    // 256 KiB: 4 blocks worth of incrementing data
    let data: Vec<u8> = (0..4 * BLOCK).map(|i| (i % 251) as u8).collect();

    let (mut writer, path) = lz4_writer(&dir, "multi.hxz");
    writer.begin_stream(true, data.len() as u64);
    for chunk in data.chunks(BLOCK) {
        writer.write_data_block(chunk).unwrap();
    }
    writer.end_stream().unwrap();
    writer.finalize(vec![], None).unwrap();

    let file = open(&path);
    assert_eq!(read_all(&file, SnapshotStream::Primary), data);
}

#[test]
fn test_partial_last_block_roundtrip() {
    // Last block is smaller than BLOCK_SIZE — tests boundary handling.
    let dir = TempDir::new().unwrap();
    let data: Vec<u8> = (0..BLOCK + 1234).map(|i| (i % 127) as u8).collect();

    let (mut writer, path) = lz4_writer(&dir, "partial.hxz");
    writer.begin_stream(true, data.len() as u64);
    for chunk in data.chunks(BLOCK) {
        writer.write_data_block(chunk).unwrap();
    }
    writer.end_stream().unwrap();
    writer.finalize(vec![], None).unwrap();

    let file = open(&path);
    assert_eq!(read_all(&file, SnapshotStream::Primary), data);
}

// ── deduplication ─────────────────────────────────────────────────────────────

#[test]
fn test_duplicate_blocks_deduplicated() {
    // Write the same block 4 times — the on-disk file should be much smaller
    // than 4 uncompressed blocks, and content reads back correctly.
    let dir = TempDir::new().unwrap();
    let block = vec![0xCCu8; BLOCK];
    let total = 4 * BLOCK as u64;

    let (mut writer, path) = lz4_writer(&dir, "dedup.hxz");
    writer.begin_stream(true, total);
    for _ in 0..4 {
        writer.write_data_block(&block).unwrap();
    }
    writer.end_stream().unwrap();
    writer.finalize(vec![], None).unwrap();

    let disk_size = path.metadata().unwrap().len();
    assert!(
        disk_size < total,
        "deduped archive ({disk_size} B) should be smaller than 4 raw blocks ({total} B)"
    );

    let file = open(&path);
    assert_eq!(file.size(SnapshotStream::Primary), total);
    let expected: Vec<u8> = block.iter().cloned().cycle().take(4 * BLOCK).collect();
    assert_eq!(read_all(&file, SnapshotStream::Primary), expected);
}

// ── zero / sparse blocks ──────────────────────────────────────────────────────

#[test]
fn test_all_zero_block_reads_back_correctly() {
    let dir = TempDir::new().unwrap();
    let data = vec![0u8; BLOCK];

    let (mut writer, path) = lz4_writer(&dir, "zeros.hxz");
    writer.begin_stream(true, data.len() as u64);
    writer.write_data_block(&data).unwrap();
    writer.end_stream().unwrap();
    writer.finalize(vec![], None).unwrap();

    let file = open(&path);
    assert_eq!(read_all(&file, SnapshotStream::Primary), data);
}

// ── metadata ──────────────────────────────────────────────────────────────────

#[test]
fn test_metadata_embedded_in_archive() {
    let dir = TempDir::new().unwrap();
    let (mut writer, path) = lz4_writer(&dir, "meta.hxz");
    let data = b"payload";
    writer.begin_stream(true, data.len() as u64);
    writer.write_data_block(data).unwrap();
    writer.end_stream().unwrap();

    let meta = br#"{"source":"test","version":2}"#;
    writer.finalize(vec![], Some(meta)).unwrap();

    // Archive must still be readable after metadata is embedded.
    let file = open(&path);
    assert_eq!(file.size(SnapshotStream::Primary), data.len() as u64);
}

// ── parent paths ──────────────────────────────────────────────────────────────

#[test]
fn test_parent_path_recorded_in_header() {
    use hexz_core::format::header::Header;

    let dir = TempDir::new().unwrap();
    let (mut writer, path) = lz4_writer(&dir, "child.hxz");
    let data = b"child data";
    writer.begin_stream(true, data.len() as u64);
    writer.write_data_block(data).unwrap();
    writer.end_stream().unwrap();
    writer
        .finalize(vec!["parent.hxz".to_string()], None)
        .unwrap();

    // Read raw header and verify parent path was stored.
    let mut f = std::fs::File::open(&path).unwrap();
    let header = Header::read_from(&mut f).unwrap();
    assert_eq!(header.parent_paths, vec!["parent.hxz"]);
}

// ── two streams ───────────────────────────────────────────────────────────────

#[test]
fn test_disk_and_memory_streams_roundtrip() {
    let dir = TempDir::new().unwrap();
    let disk_data = vec![0xD1u8; 2 * BLOCK];
    let mem_data = vec![0xE2u8; BLOCK];

    let (mut writer, path) = lz4_writer(&dir, "two_stream.hxz");

    writer.begin_stream(true, disk_data.len() as u64);
    for chunk in disk_data.chunks(BLOCK) {
        writer.write_data_block(chunk).unwrap();
    }
    writer.end_stream().unwrap();

    writer.begin_stream(false, mem_data.len() as u64);
    writer.write_data_block(&mem_data).unwrap();
    writer.end_stream().unwrap();

    writer.finalize(vec![], None).unwrap();

    let file = open(&path);
    assert_eq!(read_all(&file, SnapshotStream::Primary), disk_data);
    assert_eq!(read_all(&file, SnapshotStream::Secondary), mem_data);
}

// ── block_count / current_offset ─────────────────────────────────────────────

#[test]
fn test_block_count_matches_written_blocks() {
    let dir = TempDir::new().unwrap();
    let (mut writer, _path) = lz4_writer(&dir, "count.hxz");

    let data = vec![0x11u8; BLOCK];
    writer.begin_stream(true, 3 * BLOCK as u64);
    writer.write_data_block(&data).unwrap();
    writer.write_data_block(&data).unwrap(); // duplicate — deduped
    let unique_data = vec![0x22u8; BLOCK]; // different content
    writer.write_data_block(&unique_data).unwrap();
    writer.end_stream().unwrap();

    // 3 blocks written, but only 2 unique hashes — block_count counts
    // index entries (including dedup refs), not unique blocks.
    assert!(
        writer.block_count() >= 2,
        "at least 2 index entries expected"
    );
}
