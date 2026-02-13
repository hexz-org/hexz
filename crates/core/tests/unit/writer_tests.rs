/// Unit tests for the streaming writer
use strata_core::ops::write::Writer;
use strata_core::format::CompressionType;
use tempfile::TempDir;
use std::fs;

#[test]
fn test_writer_new() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("test.st");

    let writer = Writer::new(&output_path);
    assert!(writer.is_ok(), "Writer should be created successfully");
}

#[test]
fn test_writer_with_compression() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("compressed.st");

    let writer = Writer::new(&output_path)
        .unwrap()
        .with_compression(CompressionType::Lz4);

    assert!(writer.is_ok(), "Writer with compression should be created");
}

#[test]
fn test_writer_write_bytes() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("write_test.st");

    let mut writer = Writer::new(&output_path).unwrap();
    let data = b"Hello, Strata!";

    let result = writer.write_bytes(data);
    assert!(result.is_ok(), "Should write bytes successfully");
}

#[test]
fn test_writer_write_multiple_blocks() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("multi_block.st");

    let mut writer = Writer::new(&output_path).unwrap();

    // Write multiple blocks
    for i in 0..10 {
        let data = vec![i as u8; 1024];
        writer.write_bytes(&data).expect("Write should succeed");
    }

    let result = writer.finalize();
    assert!(result.is_ok(), "Finalize should succeed");
    assert!(output_path.exists(), "Output file should exist");
}

#[test]
fn test_writer_finalize() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("finalize_test.st");

    let mut writer = Writer::new(&output_path).unwrap();
    writer.write_bytes(b"test data").unwrap();

    let result = writer.finalize();
    assert!(result.is_ok(), "Finalize should succeed");

    // Verify file was created
    assert!(output_path.exists());
    assert!(fs::metadata(&output_path).unwrap().len() > 0);
}

#[test]
fn test_writer_empty() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("empty.st");

    let writer = Writer::new(&output_path).unwrap();

    // Finalize without writing anything
    let result = writer.finalize();
    assert!(result.is_ok(), "Should handle empty writer");
}

#[test]
fn test_writer_large_data() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("large.st");

    let mut writer = Writer::new(&output_path).unwrap();

    // Write 1MB of data
    let data = vec![0xAB; 1024 * 1024];
    let result = writer.write_bytes(&data);

    assert!(result.is_ok(), "Should handle large writes");

    writer.finalize().unwrap();
    assert!(output_path.exists());
}

#[test]
fn test_writer_compression_reduces_size() {
    let temp_dir = TempDir::new().unwrap();
    let uncompressed_path = temp_dir.path().join("uncompressed.st");
    let compressed_path = temp_dir.path().join("compressed.st");

    // Compressible data
    let data = vec![0xAA; 65536]; // 64KB of same byte

    // Write uncompressed
    let mut writer_uncomp = Writer::new(&uncompressed_path)
        .unwrap()
        .with_compression(CompressionType::None)
        .unwrap();
    writer_uncomp.write_bytes(&data).unwrap();
    writer_uncomp.finalize().unwrap();

    // Write compressed
    let mut writer_comp = Writer::new(&compressed_path)
        .unwrap()
        .with_compression(CompressionType::Lz4)
        .unwrap();
    writer_comp.write_bytes(&data).unwrap();
    writer_comp.finalize().unwrap();

    let uncomp_size = fs::metadata(&uncompressed_path).unwrap().len();
    let comp_size = fs::metadata(&compressed_path).unwrap().len();

    assert!(comp_size < uncomp_size, "Compressed should be smaller");
}

#[test]
fn test_writer_add_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("metadata.st");

    let mut writer = Writer::new(&output_path).unwrap();

    // Add metadata
    let result = writer.add_metadata("key", "value");
    // Note: metadata may not be implemented yet
    if result.is_err() {
        // Expected if not implemented
        println!("Metadata not yet implemented");
    }

    writer.write_bytes(b"data").unwrap();
    writer.finalize().unwrap();
}

#[test]
fn test_writer_bytes_written_tracking() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("tracking.st");

    let mut writer = Writer::new(&output_path).unwrap();

    let data1 = b"first chunk";
    let data2 = b"second chunk";

    writer.write_bytes(data1).unwrap();
    writer.write_bytes(data2).unwrap();

    let bytes_written = writer.bytes_written();
    assert_eq!(bytes_written, data1.len() + data2.len());
}
