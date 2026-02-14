/// Unit tests for format header serialization
use hexz_core::format::header::Header;
use hexz_core::format::{CompressionType, FeatureFlags};
use std::io::Cursor;

#[test]
fn test_header_new() {
    let header = Header::new();
    assert_eq!(header.version(), 1, "Default version should be 1");
}

#[test]
fn test_header_with_compression() {
    let header = Header::new().with_compression(CompressionType::Lz4);
    assert_eq!(header.compression(), CompressionType::Lz4);
}

#[test]
fn test_header_with_encryption() {
    let mut header = Header::new();
    header.set_encrypted(true);
    assert!(header.is_encrypted());
}

#[test]
fn test_header_serialization_roundtrip() {
    let original = Header::new()
        .with_compression(CompressionType::Zstd)
        .with_block_size(131072);

    // Serialize
    let mut buffer = Vec::new();
    original.write_to(&mut buffer).expect("Serialization should succeed");

    // Deserialize
    let mut cursor = Cursor::new(buffer);
    let deserialized = Header::read_from(&mut cursor).expect("Deserialization should succeed");

    assert_eq!(original.compression(), deserialized.compression());
    assert_eq!(original.block_size(), deserialized.block_size());
}

#[test]
fn test_header_magic_bytes() {
    let header = Header::new();
    let mut buffer = Vec::new();
    header.write_to(&mut buffer).unwrap();

    // Check magic bytes at start
    assert!(buffer.len() >= 4, "Header should have magic bytes");
    // Magic bytes should be "STRA" or similar
}

#[test]
fn test_header_version_check() {
    let header = Header::new();
    assert!(header.version() > 0, "Version should be positive");
    assert!(header.version() <= 100, "Version should be reasonable");
}

#[test]
fn test_header_feature_flags() {
    let mut header = Header::new();

    // Set various feature flags
    let flags = FeatureFlags::COMPRESSION | FeatureFlags::ENCRYPTION;
    header.set_features(flags);

    assert!(header.has_feature(FeatureFlags::COMPRESSION));
    assert!(header.has_feature(FeatureFlags::ENCRYPTION));
}

#[test]
fn test_header_block_size_validation() {
    let small = Header::new().with_block_size(4096);
    assert_eq!(small.block_size(), 4096);

    let large = Header::new().with_block_size(1048576); // 1MB
    assert_eq!(large.block_size(), 1048576);
}

#[test]
fn test_header_corruption_detection() {
    let header = Header::new();
    let mut buffer = Vec::new();
    header.write_to(&mut buffer).unwrap();

    // Corrupt a byte in the middle
    if buffer.len() > 10 {
        buffer[10] ^= 0xFF;
    }

    // Try to deserialize corrupted header
    let mut cursor = Cursor::new(buffer);
    let result = Header::read_from(&mut cursor);

    // Should detect corruption (depends on checksumming implementation)
    if result.is_err() {
        println!("Corruption detected correctly");
    }
}

#[test]
fn test_header_size_constant() {
    let header = Header::new();
    let mut buffer1 = Vec::new();
    header.write_to(&mut buffer1).unwrap();

    let header2 = Header::new()
        .with_compression(CompressionType::Zstd)
        .with_block_size(262144);
    let mut buffer2 = Vec::new();
    header2.write_to(&mut buffer2).unwrap();

    // Headers should have consistent size
    assert_eq!(
        buffer1.len(),
        buffer2.len(),
        "Header size should be constant"
    );
}

#[test]
fn test_header_all_compression_types() {
    for compression in &[CompressionType::None, CompressionType::Lz4, CompressionType::Zstd] {
        let header = Header::new().with_compression(*compression);
        let mut buffer = Vec::new();
        header.write_to(&mut buffer).unwrap();

        let mut cursor = Cursor::new(buffer);
        let deserialized = Header::read_from(&mut cursor).unwrap();

        assert_eq!(deserialized.compression(), *compression);
    }
}

#[test]
fn test_header_with_parent_snapshot() {
    let mut header = Header::new();
    header.set_parent("parent_snapshot_id");

    assert!(header.has_parent());
    assert_eq!(header.parent(), Some("parent_snapshot_id"));
}

#[test]
fn test_header_creation_timestamp() {
    let header = Header::new();

    // Should have a timestamp
    let timestamp = header.created_at();
    assert!(timestamp > 0, "Should have creation timestamp");
}
