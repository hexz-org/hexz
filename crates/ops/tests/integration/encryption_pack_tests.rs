//! Integration tests for encrypted snapshot packing and reading.
//!
//! These tests exercise the encryption code path in pack.rs and file.rs.

use super::common;
use common::*;

use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_core::algo::compression::zstd::ZstdCompressor;
use hexz_core::algo::encryption::aes_gcm::AesGcmEncryptor;
use hexz_core::format::header::Header;
use hexz_core::format::magic::HEADER_SIZE;
use hexz_core::{File, SnapshotStream};
use hexz_ops::pack::{PackConfig, pack_snapshot};
use hexz_store::local::FileBackend;
use std::fs;
use std::io::Write;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create an encrypted snapshot and read it back.
fn create_encrypted_snapshot(
    temp_dir: &TempDir,
    data: &[u8],
    password: &str,
    compression: &str,
) -> std::path::PathBuf {
    let disk_path = temp_dir.path().join("disk.img");
    fs::write(&disk_path, data).unwrap();

    let output_path = temp_dir.path().join("encrypted.hxz");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: output_path.clone(),
        compression: compression.to_string(),
        encrypt: true,
        password: Some(password.to_string()),
        train_dict: false,
        block_size: 65536,
        ..Default::default()
    };

    pack_snapshot(config, None::<fn(u64, u64)>).expect("Encrypted packing failed");
    output_path
}

/// Helper to open an encrypted snapshot.
fn open_encrypted_snapshot(path: &std::path::Path, password: &str) -> Arc<File> {
    let backend = Arc::new(FileBackend::new(path).unwrap());

    // Read header to get encryption params
    let header_bytes = backend.read_exact(0, HEADER_SIZE).unwrap();
    let header: Header = bincode::deserialize(&header_bytes).unwrap();

    let compressor: Box<dyn hexz_core::algo::compression::Compressor> = match header.compression {
        hexz_core::format::header::CompressionType::Lz4 => Box::new(Lz4Compressor::new()),
        hexz_core::format::header::CompressionType::Zstd => Box::new(ZstdCompressor::new(3, None)),
    };

    let encryptor = header.encryption.as_ref().map(|params| {
        Box::new(
            AesGcmEncryptor::new(password.as_bytes(), &params.salt, params.iterations).unwrap(),
        ) as Box<dyn hexz_core::algo::encryption::Encryptor>
    });

    File::new(backend, compressor, encryptor).unwrap()
}

use hexz_store::StorageBackend;

/// Test basic encrypted pack and read with LZ4.
#[test]
fn test_encrypted_pack_read_lz4() {
    let temp_dir = TempDir::new().unwrap();
    let data = vec![0x42u8; 256 * 1024];
    let password = "test_password_123";

    let snap_path = create_encrypted_snapshot(&temp_dir, &data, password, "lz4");
    let snapshot = open_encrypted_snapshot(&snap_path, password);

    assert_eq!(snapshot.size(SnapshotStream::Primary), 256 * 1024);

    let read_data = snapshot.read_at(SnapshotStream::Primary, 0, 4096).unwrap();
    assert_eq!(read_data.len(), 4096);
    assert!(read_data.iter().all(|&b| b == 0x42));
}

/// Test encrypted pack and read with Zstd.
#[test]
fn test_encrypted_pack_read_zstd() {
    let temp_dir = TempDir::new().unwrap();
    let data = vec![0xAB; 128 * 1024];
    let password = "zstd_secret";

    let snap_path = create_encrypted_snapshot(&temp_dir, &data, password, "zstd");
    let snapshot = open_encrypted_snapshot(&snap_path, password);

    assert_eq!(snapshot.size(SnapshotStream::Primary), 128 * 1024);

    let read_data = snapshot.read_at(SnapshotStream::Primary, 0, 1024).unwrap();
    assert!(read_data.iter().all(|&b| b == 0xAB));
}

/// Test that wrong password fails to decrypt correctly.
#[test]
fn test_encrypted_wrong_password_fails() {
    let temp_dir = TempDir::new().unwrap();
    let data = vec![0x42u8; 128 * 1024];
    let correct_password = "correct_password";
    let wrong_password = "wrong_password";

    let snap_path = create_encrypted_snapshot(&temp_dir, &data, correct_password, "lz4");

    // Opening with wrong password should fail during read (decryption error)
    let backend = Arc::new(FileBackend::new(&snap_path).unwrap());
    let header_bytes = backend.read_exact(0, HEADER_SIZE).unwrap();
    let header: Header = bincode::deserialize(&header_bytes).unwrap();

    let encryptor = header.encryption.as_ref().map(|params| {
        Box::new(
            AesGcmEncryptor::new(wrong_password.as_bytes(), &params.salt, params.iterations)
                .unwrap(),
        ) as Box<dyn hexz_core::algo::encryption::Encryptor>
    });

    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = File::new(backend, compressor, encryptor).unwrap();

    // Reading should fail because decryption with wrong key produces garbage
    let result = snapshot.read_at(SnapshotStream::Primary, 0, 4096);
    assert!(result.is_err(), "Wrong password should cause read failure");
}

/// Test encrypted snapshot with varied data patterns.
#[test]
fn test_encrypted_varied_data() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");

    let mut file = fs::File::create(&disk_path).unwrap();
    for i in 0..8 {
        let block = vec![(i * 37) as u8; 65536];
        file.write_all(&block).unwrap();
    }
    drop(file);

    let output_path = temp_dir.path().join("encrypted.hxz");
    let password = "varied_data_pw";

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: output_path.clone(),
        compression: "lz4".to_string(),
        encrypt: true,
        password: Some(password.to_string()),
        train_dict: false,
        block_size: 65536,
        ..Default::default()
    };

    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    let snapshot = open_encrypted_snapshot(&output_path, password);

    for i in 0..8u64 {
        let expected = (i * 37) as u8;
        let read = snapshot
            .read_at(SnapshotStream::Primary, i * 65536, 1024)
            .unwrap();
        assert!(
            read.iter().all(|&b| b == expected),
            "Block {} mismatch: expected 0x{:02X}",
            i,
            expected
        );
    }
}

/// Test encrypted snapshot with both disk and memory.
#[test]
fn test_encrypted_dual_stream() {
    let temp_dir = TempDir::new().unwrap();

    let disk_path = temp_dir.path().join("disk.img");
    let mem_path = temp_dir.path().join("mem.img");
    fs::write(&disk_path, vec![0xDD; 256 * 1024]).unwrap();
    fs::write(&mem_path, vec![0xCC; 128 * 1024]).unwrap();

    let output_path = temp_dir.path().join("encrypted.hxz");
    let password = "dual_stream_pw";

    let config = PackConfig {
        disk: Some(disk_path),
        memory: Some(mem_path),
        output: output_path.clone(),
        compression: "lz4".to_string(),
        encrypt: true,
        password: Some(password.to_string()),
        train_dict: false,
        block_size: 65536,
        ..Default::default()
    };

    pack_snapshot(config, None::<fn(u64, u64)>).unwrap();

    let snapshot = open_encrypted_snapshot(&output_path, password);

    assert_eq!(snapshot.size(SnapshotStream::Primary), 256 * 1024);
    assert_eq!(snapshot.size(SnapshotStream::Secondary), 128 * 1024);

    let disk_read = snapshot.read_at(SnapshotStream::Primary, 0, 1024).unwrap();
    assert!(disk_read.iter().all(|&b| b == 0xDD));

    let mem_read = snapshot
        .read_at(SnapshotStream::Secondary, 0, 1024)
        .unwrap();
    assert!(mem_read.iter().all(|&b| b == 0xCC));
}

/// Test encrypted pack without password fails.
#[test]
fn test_encrypted_pack_no_password_fails() {
    let temp_dir = TempDir::new().unwrap();
    let disk_path = temp_dir.path().join("disk.img");
    fs::write(&disk_path, vec![0u8; 65536]).unwrap();

    let output_path = temp_dir.path().join("no_pw.hxz");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: output_path,
        compression: "lz4".to_string(),
        encrypt: true,
        password: None, // Missing password
        train_dict: false,
        block_size: 65536,
        ..Default::default()
    };

    let result = pack_snapshot(config, None::<fn(u64, u64)>);
    assert!(result.is_err(), "Should fail without password");
}

/// Test reading full encrypted file sequentially.
#[test]
fn test_encrypted_sequential_read() {
    let temp_dir = TempDir::new().unwrap();
    let data: Vec<u8> = (0..256 * 1024).map(|i| (i % 256) as u8).collect();
    let password = "sequential_pw";

    let snap_path = create_encrypted_snapshot(&temp_dir, &data, password, "lz4");
    let snapshot = open_encrypted_snapshot(&snap_path, password);

    // Read in chunks and verify
    let mut offset = 0u64;
    let chunk_size = 32768;
    while offset < data.len() as u64 {
        let remaining = data.len() as u64 - offset;
        let to_read = std::cmp::min(chunk_size, remaining as usize);
        let chunk = snapshot
            .read_at(SnapshotStream::Primary, offset, to_read)
            .unwrap();
        assert_eq!(
            &chunk[..],
            &data[offset as usize..offset as usize + to_read],
            "Mismatch at offset {}",
            offset
        );
        offset += to_read as u64;
    }
}

/// Test encrypted snapshot with random data (incompressible).
#[test]
fn test_encrypted_random_data() {
    let temp_dir = TempDir::new().unwrap();
    let data = create_random_data(128 * 1024);
    let password = "random_pw";

    let snap_path = create_encrypted_snapshot(&temp_dir, &data, password, "lz4");
    let snapshot = open_encrypted_snapshot(&snap_path, password);

    let read_data = snapshot
        .read_at(SnapshotStream::Primary, 0, data.len())
        .unwrap();
    assert_bytes_equal(&read_data, &data, "encrypted random data round-trip");
}
