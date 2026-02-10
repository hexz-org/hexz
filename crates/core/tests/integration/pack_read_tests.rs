//! Integration tests for snapshot packing and reading.
//!
//! These tests verify end-to-end functionality of creating snapshots
//! and reading them back with various configurations.

use super::common;
use common::*;

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use strata_core::algo::compression::lz4::Lz4Compressor;
use strata_core::algo::compression::zstd::ZstdCompressor;
use strata_core::ops::pack::{PackConfig, pack_snapshot};
use strata_core::store::local::FileBackend;
use strata_core::{SnapshotStream, StrataFile};
use tempfile::TempDir;

/// Helper to create a test file with specific content.
fn create_test_file(dir: &TempDir, name: &str, size: usize, pattern: u8) -> PathBuf {
    let path = dir.path().join(name);
    let data = vec![pattern; size];
    fs::write(&path, data).unwrap();
    path
}

/// Test basic snapshot creation and reading with LZ4 compression.
#[test]
fn test_pack_and_read_lz4() {
    let temp_dir = TempDir::new().unwrap();

    // Create a 1MB test disk image
    let disk_path = create_test_file(&temp_dir, "disk.img", 1024 * 1024, 0x42);
    let output_path = temp_dir.path().join("snapshot.st");

    // Pack the snapshot
    let config = PackConfig {
        disk: Some(disk_path.clone()),
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
    };

    pack_snapshot(config, None::<fn(u64, u64)>).expect("Packing failed");

    // Verify snapshot was created
    assert!(output_path.exists());
    let snapshot_size = fs::metadata(&output_path).unwrap().len();
    assert!(snapshot_size > 0);

    // Read back the snapshot
    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    // Verify size
    assert_eq!(snapshot.size(SnapshotStream::Disk), 1024 * 1024);

    // Read and verify data
    let data = snapshot.read_at(SnapshotStream::Disk, 0, 4096).unwrap();
    assert_eq!(data.len(), 4096);
    assert!(data.iter().all(|&b| b == 0x42));

    // Read from middle
    let middle_data = snapshot
        .read_at(SnapshotStream::Disk, 512 * 1024, 4096)
        .unwrap();
    assert_eq!(middle_data.len(), 4096);
    assert!(middle_data.iter().all(|&b| b == 0x42));
}

/// Test snapshot with both disk and memory streams.
#[test]
fn test_pack_disk_and_memory() {
    let temp_dir = TempDir::new().unwrap();

    // Create test files
    let disk_path = create_test_file(&temp_dir, "disk.img", 512 * 1024, 0xAA);
    let memory_path = create_test_file(&temp_dir, "memory.dump", 256 * 1024, 0xBB);
    let output_path = temp_dir.path().join("snapshot.st");

    // Pack both streams
    let config = PackConfig {
        disk: Some(disk_path),
        memory: Some(memory_path),
        output: output_path.clone(),
        compression: "lz4".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };

    pack_snapshot(config, None::<fn(u64, u64)>).expect("Packing failed");

    // Read back
    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    // Verify both stream sizes
    assert_eq!(snapshot.size(SnapshotStream::Disk), 512 * 1024);
    assert_eq!(snapshot.size(SnapshotStream::Memory), 256 * 1024);

    // Verify disk stream data
    let disk_data = snapshot.read_at(SnapshotStream::Disk, 0, 1024).unwrap();
    assert!(disk_data.iter().all(|&b| b == 0xAA));

    // Verify memory stream data
    let mem_data = snapshot.read_at(SnapshotStream::Memory, 0, 1024).unwrap();
    assert!(mem_data.iter().all(|&b| b == 0xBB));
}

/// Test snapshot with varying data patterns.
#[test]
fn test_pack_varied_data() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");

    // Create file with varying data patterns
    let mut file = fs::File::create(&disk_path).unwrap();

    // Write different patterns in 64KB blocks
    for i in 0..16 {
        let pattern = (i * 17) as u8;
        let block = vec![pattern; 65536];
        file.write_all(&block).unwrap();
    }
    drop(file);

    let output_path = temp_dir.path().join("snapshot.st");

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

    pack_snapshot(config, None::<fn(u64, u64)>).expect("Packing failed");

    // Read back and verify
    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    // Verify each block has the correct pattern
    for i in 0..16 {
        let expected_pattern = (i * 17) as u8;
        let offset = i * 65536;
        let data = snapshot
            .read_at(SnapshotStream::Disk, offset, 1024)
            .unwrap();
        assert!(
            data.iter().all(|&b| b == expected_pattern),
            "Block {} has wrong pattern",
            i
        );
    }
}

/// Test reading at various offsets and lengths.
#[test]
fn test_random_access_patterns() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 1024 * 1024, 0x55);
    let output_path = temp_dir.path().join("snapshot.st");

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

    pack_snapshot(config, None::<fn(u64, u64)>).expect("Packing failed");

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    // Test various read patterns
    let test_cases = vec![
        (0, 100),                 // Start
        (1024, 2048),             // Aligned
        (1111, 333),              // Unaligned
        (512 * 1024, 4096),       // Middle
        (1024 * 1024 - 100, 100), // End
        (0, 1024 * 1024),         // Full read
    ];

    for (offset, length) in test_cases {
        let data = snapshot
            .read_at(SnapshotStream::Disk, offset, length)
            .unwrap();
        assert_eq!(
            data.len(),
            length,
            "Read at {} with length {} failed",
            offset,
            length
        );
        assert!(data.iter().all(|&b| b == 0x55));
    }
}

/// Test read beyond end of stream.
#[test]
fn test_read_beyond_end() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 4096, 0x77);
    let output_path = temp_dir.path().join("snapshot.st");

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

    pack_snapshot(config, None::<fn(u64, u64)>).expect("Packing failed");

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    // Read starting beyond end should return empty
    let data = snapshot.read_at(SnapshotStream::Disk, 10000, 1000).unwrap();
    assert_eq!(data.len(), 0);

    // Read overlapping end should return partial data
    let data = snapshot.read_at(SnapshotStream::Disk, 4000, 1000).unwrap();
    assert_eq!(data.len(), 96); // Only 96 bytes available
}

/// Test empty snapshot (edge case).
#[test]
fn test_empty_snapshot() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 0, 0);
    let output_path = temp_dir.path().join("snapshot.st");

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

    pack_snapshot(config, None::<fn(u64, u64)>).expect("Packing failed");

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    assert_eq!(snapshot.size(SnapshotStream::Disk), 0);

    let data = snapshot.read_at(SnapshotStream::Disk, 0, 1000).unwrap();
    assert_eq!(data.len(), 0);
}

/// Test small snapshot (smaller than one block).
#[test]
fn test_small_snapshot() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 512, 0x88);
    let output_path = temp_dir.path().join("snapshot.st");

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

    pack_snapshot(config, None::<fn(u64, u64)>).expect("Packing failed");

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    assert_eq!(snapshot.size(SnapshotStream::Disk), 512);

    let data = snapshot.read_at(SnapshotStream::Disk, 0, 512).unwrap();
    assert_eq!(data.len(), 512);
    assert!(data.iter().all(|&b| b == 0x88));
}

// Compression variant tests

#[test]
fn test_pack_with_zstd_level1() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 1024 * 1024, 0x42);
    let output_path = temp_dir.path().join("snapshot.st");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: output_path.clone(),
        compression: "zstd".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };

    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(ZstdCompressor::new(1, None));
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    assert_eq!(snapshot.size(SnapshotStream::Disk), 1024 * 1024);
    let data = snapshot.read_at(SnapshotStream::Disk, 0, 4096).unwrap();
    assert!(data.iter().all(|&b| b == 0x42));
}

#[test]
fn test_pack_with_zstd_level9() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 512 * 1024, 0xAB);
    let output_path = temp_dir.path().join("snapshot.st");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: output_path.clone(),
        compression: "zstd".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };

    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(ZstdCompressor::new(9, None));
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    let data = snapshot.read_at(SnapshotStream::Disk, 0, 512).unwrap();
    assert_eq!(data.len(), 512);
}

#[test]
fn test_pack_with_zstd_dictionary() {
    let temp_dir = TempDir::new().unwrap();

    // Create structured data that will benefit from dictionary
    // Need enough data for dictionary training to work
    let disk_path = temp_dir.path().join("disk.img");
    let mut file = fs::File::create(&disk_path).unwrap();
    let pattern = b"This is a repeating pattern for dictionary training. ";
    for _ in 0..20000 {
        // Enough samples for training
        file.write_all(pattern).unwrap();
    }
    drop(file);

    let output_path = temp_dir.path().join("snapshot.st");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: output_path.clone(),
        compression: "zstd".to_string(),
        encrypt: false,
        password: None,
        train_dict: true, // Enable dictionary training
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };

    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    // Note: Reading with dictionary requires the dict to be embedded in snapshot
    // For now, just verify the snapshot was created
    assert!(output_path.exists());
}

// Block size variation tests

#[test]
fn test_pack_with_4kb_blocks() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 256 * 1024, 0x11);
    let output_path = temp_dir.path().join("snapshot.st");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: output_path.clone(),
        compression: "lz4".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 4096, // Small blocks
        cdc_enabled: false,
        ..Default::default()
    };

    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    let data = snapshot
        .read_at(SnapshotStream::Disk, 0, 256 * 1024)
        .unwrap();
    assert_eq!(data.len(), 256 * 1024);
}

#[test]
fn test_pack_with_256kb_blocks() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 2 * 1024 * 1024, 0x22);
    let output_path = temp_dir.path().join("snapshot.st");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: output_path.clone(),
        compression: "lz4".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 256 * 1024, // Large blocks
        cdc_enabled: false,
        ..Default::default()
    };

    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    assert_eq!(snapshot.size(SnapshotStream::Disk), 2 * 1024 * 1024);
}

#[test]
fn test_pack_with_1mb_blocks() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 5 * 1024 * 1024, 0x33);
    let output_path = temp_dir.path().join("snapshot.st");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: output_path.clone(),
        compression: "lz4".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 1024 * 1024, // 1MB blocks
        cdc_enabled: false,
        ..Default::default()
    };

    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    let data = snapshot
        .read_at(SnapshotStream::Disk, 2 * 1024 * 1024, 1024)
        .unwrap();
    assert_eq!(data.len(), 1024);
}

// Data pattern tests

#[test]
fn test_pack_random_data() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");

    let random_data = create_random_data(512 * 1024);
    fs::write(&disk_path, &random_data).unwrap();

    let output_path = temp_dir.path().join("snapshot.st");

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

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    let data = snapshot
        .read_at(SnapshotStream::Disk, 0, 512 * 1024)
        .unwrap();
    assert_bytes_equal(&data, &random_data, "random data round-trip");
}

#[test]
fn test_pack_sparse_data() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");

    let sparse_data = create_sparse_data(1024 * 1024, 0.95); // 95% zeros
    fs::write(&disk_path, &sparse_data).unwrap();

    let output_path = temp_dir.path().join("snapshot.st");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: output_path.clone(),
        compression: "zstd".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };

    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(ZstdCompressor::new(3, None));
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    let data = snapshot
        .read_at(SnapshotStream::Disk, 0, 1024 * 1024)
        .unwrap();
    assert_bytes_equal(&data, &sparse_data, "sparse data round-trip");
}

#[test]
fn test_pack_structured_data() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");

    let structured_data = create_structured_data(512 * 1024, 1024);
    fs::write(&disk_path, &structured_data).unwrap();

    let output_path = temp_dir.path().join("snapshot.st");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: output_path.clone(),
        compression: "zstd".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        ..Default::default()
    };

    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(ZstdCompressor::new(3, None));
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    let data = snapshot
        .read_at(SnapshotStream::Disk, 100000, 10000)
        .unwrap();
    assert_bytes_equal(&data, &structured_data[100000..110000], "structured data");
}

// Large file tests

#[test]
fn test_pack_10mb_file() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 10 * 1024 * 1024, 0x99);
    let output_path = temp_dir.path().join("snapshot.st");

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

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    assert_eq!(snapshot.size(SnapshotStream::Disk), 10 * 1024 * 1024);

    // Read from start
    let data = snapshot.read_at(SnapshotStream::Disk, 0, 4096).unwrap();
    assert!(data.iter().all(|&b| b == 0x99));

    // Read from middle
    let data = snapshot
        .read_at(SnapshotStream::Disk, 5 * 1024 * 1024, 4096)
        .unwrap();
    assert!(data.iter().all(|&b| b == 0x99));
}

#[test]
#[ignore] // Slow test - only run when needed
fn test_pack_100mb_file() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 100 * 1024 * 1024, 0xAA);
    let output_path = temp_dir.path().join("snapshot.st");

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

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    assert_eq!(snapshot.size(SnapshotStream::Disk), 100 * 1024 * 1024);
}

// Sequential read tests

#[test]
fn test_sequential_reads_full_file() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");

    let test_data = create_random_data(512 * 1024);
    fs::write(&disk_path, &test_data).unwrap();

    let output_path = temp_dir.path().join("snapshot.st");

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

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    // Read entire file sequentially in 64KB chunks
    let mut reconstructed = Vec::new();
    let chunk_size = 65536;
    let mut offset = 0;

    while offset < test_data.len() as u64 {
        let remaining = test_data.len() as u64 - offset;
        let to_read = std::cmp::min(chunk_size, remaining as usize);

        let chunk = snapshot
            .read_at(SnapshotStream::Disk, offset, to_read)
            .unwrap();
        reconstructed.extend_from_slice(&chunk);
        offset += chunk.len() as u64;
    }

    assert_bytes_equal(&reconstructed, &test_data, "sequential reconstruction");
}

// Dual stream tests

#[test]
fn test_pack_large_disk_small_memory() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 2 * 1024 * 1024, 0xDD);
    let memory_path = create_test_file(&temp_dir, "memory.dump", 64 * 1024, 0xCC);
    let output_path = temp_dir.path().join("snapshot.st");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: Some(memory_path),
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

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    assert_eq!(snapshot.size(SnapshotStream::Disk), 2 * 1024 * 1024);
    assert_eq!(snapshot.size(SnapshotStream::Memory), 64 * 1024);
}

#[test]
fn test_pack_equal_disk_and_memory() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 1024 * 1024, 0xEE);
    let memory_path = create_test_file(&temp_dir, "memory.dump", 1024 * 1024, 0xFF);
    let output_path = temp_dir.path().join("snapshot.st");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: Some(memory_path),
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

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    let disk_data = snapshot.read_at(SnapshotStream::Disk, 0, 1024).unwrap();
    let mem_data = snapshot.read_at(SnapshotStream::Memory, 0, 1024).unwrap();

    assert!(disk_data.iter().all(|&b| b == 0xEE));
    assert!(mem_data.iter().all(|&b| b == 0xFF));
}

// Compression ratio verification tests

#[test]
fn test_compression_ratio_zeros() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 1024 * 1024, 0x00);
    let output_path = temp_dir.path().join("snapshot.st");

    let config = PackConfig {
        disk: Some(disk_path.clone()),
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

    let original_size = fs::metadata(&disk_path).unwrap().len();
    let compressed_size = fs::metadata(&output_path).unwrap().len();

    // All zeros should compress extremely well (ratio < 1%)
    let ratio = compressed_size as f64 / original_size as f64;
    assert!(
        ratio < 0.01,
        "Compression ratio should be <1% for all zeros"
    );
}

// Edge case tests

#[test]
fn test_pack_file_not_multiple_of_block_size() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 100000, 0xBB); // Not a multiple of 65536
    let output_path = temp_dir.path().join("snapshot.st");

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

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    assert_eq!(snapshot.size(SnapshotStream::Disk), 100000);
    let data = snapshot.read_at(SnapshotStream::Disk, 0, 100000).unwrap();
    assert_eq!(data.len(), 100000);
}

#[test]
fn test_pack_single_block_file() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = create_test_file(&temp_dir, "disk.img", 32768, 0xCC); // Half a block
    let output_path = temp_dir.path().join("snapshot.st");

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

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    let data = snapshot.read_at(SnapshotStream::Disk, 0, 32768).unwrap();
    assert!(data.iter().all(|&b| b == 0xCC));
}

#[test]
fn test_pack_verify_all_patterns() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");

    // Create file with alternating patterns
    let mut file = fs::File::create(&disk_path).unwrap();
    for i in 0..8 {
        let pattern = vec![(i * 31) as u8; 65536];
        file.write_all(&pattern).unwrap();
    }
    drop(file);

    let output_path = temp_dir.path().join("snapshot.st");

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

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None).unwrap();

    // Verify each block
    for i in 0..8 {
        let expected_pattern = (i * 31) as u8;
        let offset = i * 65536;
        let data = snapshot
            .read_at(SnapshotStream::Disk, offset, 65536)
            .unwrap();
        verify_pattern(&data, expected_pattern);
    }
}
