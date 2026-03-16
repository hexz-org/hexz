//! Integration tests for CDC (Content-Defined Chunking) packing.
//!
//! These tests exercise the CDC code path in pack.rs.

use super::common;
use common::*;

use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_core::algo::compression::zstd::ZstdCompressor;
use hexz_core::{Archive, ArchiveStream};
use hexz_ops::pack::{PackAnalysisFlags, PackConfig, PackTransformFlags, pack_archive};
use hexz_store::local::FileBackend;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

/// Test CDC packing with LZ4 compression.
#[test]
fn test_cdc_pack_lz4() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");

    // Create data with some repetitive patterns that CDC can chunk well
    let mut data = Vec::new();
    for i in 0..16 {
        let block = vec![(i * 17) as u8; 65536];
        data.extend_from_slice(&block);
    }
    fs::write(&disk_path, &data).unwrap();

    let output_path = temp_dir.path().join("cdc.hxz");

    let config = PackConfig {
        input: disk_path,
        output: output_path.clone(),
        compression: "lz4".to_string(),
        password: None,
        block_size: 65536,
        min_chunk: Some(16384),
        avg_chunk: Some(65536),
        max_chunk: Some(131072),
        ..Default::default()
    };

    pack_archive(&config, None::<&fn(u64, u64)>).expect("CDC packing failed");

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let archive = Archive::new(backend, compressor, None).unwrap();

    assert_eq!(archive.size(ArchiveStream::Main) as usize, data.len());

    // Verify data integrity
    let read_data = archive.read_at(ArchiveStream::Main, 0, data.len()).unwrap();
    assert_bytes_equal(&read_data, &data, "CDC LZ4 round-trip");
}

/// Test CDC packing with Zstd compression.
#[test]
fn test_cdc_pack_zstd() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");

    let data: Vec<u8> = (0..512 * 1024).map(|i| (i % 256) as u8).collect();
    fs::write(&disk_path, &data).unwrap();

    let output_path = temp_dir.path().join("cdc_zstd.hxz");

    let config = PackConfig {
        input: disk_path,
        output: output_path.clone(),
        compression: "zstd".to_string(),
        password: None,
        block_size: 65536,
        min_chunk: Some(16384),
        avg_chunk: Some(65536),
        max_chunk: Some(131072),
        ..Default::default()
    };

    pack_archive(&config, None::<&fn(u64, u64)>).unwrap();

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(ZstdCompressor::new(3, None));
    let archive = Archive::new(backend, compressor, None).unwrap();

    let read_data = archive.read_at(ArchiveStream::Main, 0, data.len()).unwrap();
    assert_bytes_equal(&read_data, &data, "CDC Zstd round-trip");
}

/// Test CDC with encrypted archive.
#[test]
fn test_cdc_encrypted() {
    use hexz_core::algo::encryption::aes_gcm::AesGcmEncryptor;
    use hexz_core::format::header::Header;
    use hexz_core::format::magic::HEADER_SIZE;
    use hexz_store::StorageBackend;

    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");

    let data = vec![0xAA; 256 * 1024];
    fs::write(&disk_path, &data).unwrap();

    let output_path = temp_dir.path().join("cdc_enc.hxz");
    let password = "cdc_encrypt_pw";

    let config = PackConfig {
        input: disk_path,
        output: output_path.clone(),
        compression: "lz4".to_string(),
        password: Some(password.to_string()),
        transform: PackTransformFlags {
            encrypt: true,
            train_dict: false,
            ..Default::default()
        },
        block_size: 65536,
        min_chunk: Some(16384),
        avg_chunk: Some(65536),
        max_chunk: Some(131072),
        ..Default::default()
    };

    pack_archive(&config, None::<&fn(u64, u64)>).unwrap();

    // Read back with encryption
    let backend = Arc::new(FileBackend::new(&output_path).unwrap());

    let header_bytes = backend.read_exact(0, HEADER_SIZE).unwrap();
    let header: Header = bincode::deserialize(&header_bytes).unwrap();

    let encryptor = header.encryption.as_ref().map(|params| {
        Box::new(
            AesGcmEncryptor::new(password.as_bytes(), &params.salt, params.iterations).unwrap(),
        ) as Box<dyn hexz_core::algo::encryption::Encryptor>
    });

    let compressor = Box::new(Lz4Compressor::new());
    let archive = Archive::new(backend, compressor, encryptor).unwrap();

    let read_data = archive.read_at(ArchiveStream::Main, 0, 4096).unwrap();
    assert!(read_data.iter().all(|&b| b == 0xAA));
}

/// Test CDC with small chunks.
#[test]
fn test_cdc_small_chunks() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");

    let data = create_random_data(256 * 1024);
    fs::write(&disk_path, &data).unwrap();

    let output_path = temp_dir.path().join("cdc_small.hxz");

    let config = PackConfig {
        input: disk_path,
        output: output_path.clone(),
        compression: "lz4".to_string(),
        password: None,
        block_size: 65536,
        min_chunk: Some(4096),
        avg_chunk: Some(16384),
        max_chunk: Some(65536),
        ..Default::default()
    };

    pack_archive(&config, None::<&fn(u64, u64)>).unwrap();

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let archive = Archive::new(backend, compressor, None).unwrap();

    let read_data = archive.read_at(ArchiveStream::Main, 0, data.len()).unwrap();
    assert_bytes_equal(&read_data, &data, "CDC small chunks round-trip");
}

/// Test CDC with deduplication (repetitive data).
#[test]
fn test_cdc_deduplication() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");

    // Create highly repetitive data that should dedup well
    let pattern = vec![0x42u8; 65536];
    let mut data = Vec::new();
    for _ in 0..16 {
        data.extend_from_slice(&pattern);
    }
    fs::write(&disk_path, &data).unwrap();

    let output_path = temp_dir.path().join("cdc_dedup.hxz");

    let config = PackConfig {
        input: disk_path.clone(),
        output: output_path.clone(),
        compression: "lz4".to_string(),
        password: None,
        block_size: 65536,
        min_chunk: Some(16384),
        avg_chunk: Some(65536),
        max_chunk: Some(131072),
        ..Default::default()
    };

    pack_archive(&config, None::<&fn(u64, u64)>).unwrap();

    // The compressed file should be significantly smaller than the original
    let original_size = fs::metadata(&disk_path).unwrap().len();
    let compressed_size = fs::metadata(&output_path).unwrap().len();
    assert!(
        compressed_size < original_size / 2,
        "Dedup should significantly reduce size: {compressed_size} vs {original_size}"
    );

    // Verify data integrity
    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let archive = Archive::new(backend, compressor, None).unwrap();

    let read_data = archive.read_at(ArchiveStream::Main, 0, data.len()).unwrap();
    assert_bytes_equal(&read_data, &data, "CDC dedup round-trip");
}

/// Test CDC with dual streams.
#[test]
fn test_cdc_dual_stream() {
    let temp_dir = TempDir::new().unwrap();
    let input_dir = temp_dir.path().join("input");
    fs::create_dir(&input_dir).unwrap();

    let disk_data = vec![0xDD; 256 * 1024];
    let mem_data = vec![0xCC; 128 * 1024];
    fs::write(input_dir.join("disk"), &disk_data).unwrap();
    fs::write(input_dir.join("memory"), &mem_data).unwrap();

    let output_path = temp_dir.path().join("cdc_dual.hxz");

    let config = PackConfig {
        input: input_dir,
        output: output_path.clone(),
        compression: "lz4".to_string(),
        password: None,
        block_size: 65536,
        min_chunk: Some(16384),
        avg_chunk: Some(65536),
        max_chunk: Some(131072),
        ..Default::default()
    };

    pack_archive(&config, None::<&fn(u64, u64)>).unwrap();

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let archive = Archive::new(backend, compressor, None).unwrap();

    assert_eq!(archive.size(ArchiveStream::Main), 256 * 1024);
    assert_eq!(archive.size(ArchiveStream::Auxiliary), 128 * 1024);

    let disk_read = archive.read_at(ArchiveStream::Main, 0, 1024).unwrap();
    assert!(disk_read.iter().all(|&b| b == 0xDD));

    let mem_read = archive.read_at(ArchiveStream::Auxiliary, 0, 1024).unwrap();
    assert!(mem_read.iter().all(|&b| b == 0xCC));
}

/// Test packing with progress callback.
#[test]
fn test_pack_with_progress_callback() {
    use std::sync::atomic::{AtomicU64, Ordering};

    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");
    fs::write(&disk_path, vec![0u8; 256 * 1024]).unwrap();

    let output_path = temp_dir.path().join("progress.hxz");
    let last_progress = Arc::new(AtomicU64::new(0));
    let callback_count = Arc::new(AtomicU64::new(0));

    let lp = last_progress;
    let cc = callback_count.clone();

    let config = PackConfig {
        input: disk_path,
        output: output_path,
        compression: "lz4".to_string(),
        password: None,
        block_size: 65536,
        ..Default::default()
    };

    let cb = move |current: u64, total: u64| {
        lp.store(current, Ordering::SeqCst);
        cc.fetch_add(1, Ordering::SeqCst);
        assert!(total > 0);
        assert!(current <= total);
    };
    pack_archive(&config, Some(&cb)).unwrap();

    assert!(
        callback_count.load(Ordering::SeqCst) > 0,
        "Progress callback should have been called"
    );
}

/// Test CDC packing with DCAM and Zstd for high compression.
#[test]
fn test_cdc_dcam_zstd_high_compression() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("cdc_dcam.hxz");

    // 1. Generate highly redundant data
    // 1MB of repeating patterns + some unique data
    let mut data = Vec::with_capacity(1024 * 1024 + 24 * 1024);
    let pattern = vec![0xAA; 1024]; // 1KB pattern
    for _ in 0..1024 {
        data.extend_from_slice(&pattern);
    }

    // Add 24KB of semi-random data (using a predictable loop for simplicity without extra deps)
    for i in 0..24 * 1024 {
        data.push((i % 251) as u8);
    }

    let input_path = temp_dir.path().join("input.bin");
    fs::write(&input_path, &data).unwrap();

    // 2. Pack with CDC + DCAM + Zstd
    let config = PackConfig {
        input: input_path,
        output: output_path.clone(),
        compression: "zstd".to_string(),
        min_chunk: None, // Let DCAM decide
        avg_chunk: None,
        max_chunk: None,
        analysis: PackAnalysisFlags {
            use_dcam: true,
            ..Default::default()
        },
        ..Default::default()
    };

    pack_archive(&config, None::<&fn(u64, u64)>).expect("CDC+DCAM packing failed");

    // 3. Verify high compression/deduplication
    let metadata = fs::metadata(&output_path).unwrap();
    let archive_size = metadata.len();

    // Original size is ~1048 KB.
    // With 1024x redundancy, the pattern part should dedup to ~1KB compressed.
    // Unique data is 24KB.
    // Expected size should be well under 100KB including metadata.
    assert!(
        archive_size < 100 * 1024,
        "Archive size too large: {archive_size} bytes"
    );

    // 4. Verify Integrity
    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(ZstdCompressor::new(3, None));
    let archive = Archive::new(backend, compressor, None).unwrap();

    assert_eq!(archive.size(ArchiveStream::Main) as usize, data.len());
    let read_data = archive.read_at(ArchiveStream::Main, 0, data.len()).unwrap();
    assert_eq!(read_data, data);
}
