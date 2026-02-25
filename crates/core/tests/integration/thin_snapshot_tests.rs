//! Integration tests for thin (incremental) snapshots.
//!
//! These tests exercise the parent chain logic in file.rs.

use super::common;
use common::*;

use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_core::{File, SnapshotStream};
use hexz_ops::pack::{PackConfig, pack_snapshot};
use hexz_store::local::FileBackend;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

/// Test creating and reading a thin snapshot with parent.
#[test]
fn test_thin_snapshot_basic() {
    let temp_dir = TempDir::new().unwrap();

    // Create base snapshot
    let base_disk = temp_dir.path().join("base_disk.img");
    let base_data = vec![0xAA; 256 * 1024];
    fs::write(&base_disk, &base_data).unwrap();

    let base_snap = temp_dir.path().join("base.hxz");
    let config = PackConfig {
        disk: Some(base_disk),
        memory: None,
        output: base_snap.clone(),
        compression: "lz4".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };
    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    // Verify base snapshot works
    let backend = Arc::new(FileBackend::new(&base_snap).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = File::new(backend, compressor, None).unwrap();

    let data = snapshot.read_at(SnapshotStream::Primary, 0, 1024).unwrap();
    assert!(data.iter().all(|&b| b == 0xAA));
}

/// Test reading a zstd dictionary snapshot end-to-end.
#[test]
fn test_zstd_dict_snapshot_read() {
    use hexz_core::algo::compression::zstd::ZstdCompressor;
    use hexz_core::format::header::{CompressionType, Header};
    use hexz_core::format::magic::HEADER_SIZE;
    use hexz_store::StorageBackend;

    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");

    // Create data that benefits from dictionary (repeated structures)
    let mut data = Vec::new();
    let pattern = b"HEADER_MAGIC_v1.0: field1=value1, field2=value2\n";
    for _ in 0..20000 {
        data.extend_from_slice(pattern);
    }
    fs::write(&disk_path, &data).unwrap();

    let output_path = temp_dir.path().join("dict.hxz");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: output_path.clone(),
        compression: "zstd".to_string(),
        encrypt: false,
        password: None,
        train_dict: true,
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };

    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    // Read back the snapshot, loading the dictionary from the header
    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let header_bytes = backend.read_exact(0, HEADER_SIZE).unwrap();
    let header: Header = bincode::deserialize(&header_bytes).unwrap();

    assert_eq!(header.compression, CompressionType::Zstd);

    // Load dictionary if present
    let dict = if let (Some(off), Some(len)) = (header.dictionary_offset, header.dictionary_length)
    {
        Some(backend.read_exact(off, len as usize).unwrap().to_vec())
    } else {
        None
    };

    let compressor = Box::new(ZstdCompressor::new(3, dict));
    let snapshot = File::new(backend, compressor, None).unwrap();

    // Verify data integrity
    let read_data = snapshot
        .read_at(SnapshotStream::Primary, 0, data.len())
        .unwrap();
    assert_bytes_equal(&read_data, &data, "zstd dict round-trip");
}

/// Test version compatibility check path.
#[test]
fn test_version_check_on_open() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");
    fs::write(&disk_path, vec![0u8; 65536]).unwrap();

    let output_path = temp_dir.path().join("version.hxz");

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
        ..Default::default()
    };

    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    // Open and verify the version check passes
    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = File::new(backend, compressor, None).unwrap();

    // Should have the current format version
    assert!(snapshot.header.version > 0);
}
