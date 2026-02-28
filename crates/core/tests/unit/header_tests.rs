/// Unit tests for Header::read_from() and format-level roundtrips.
///
/// The inline tests in `format/header.rs` cover field access and bincode
/// serialize/deserialize directly.  These tests exercise the public
/// `Header::read_from()` path — the same code path used by every reader in
/// production — and boundary conditions that are easier to express here than
/// in a `#[cfg(test)]` block inside the module.
use hexz_core::format::header::{CompressionType, FeatureFlags, Header};
use hexz_core::format::magic::{HEADER_SIZE, MAGIC_BYTES};
use std::io::Cursor;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Serialize `header` into a zero-padded `HEADER_SIZE`-byte buffer, then
/// deserialize it back via `Header::read_from()`.
fn roundtrip(header: &Header) -> Header {
    let serialized = bincode::serialize(header).unwrap();
    assert!(
        serialized.len() <= HEADER_SIZE,
        "serialized header ({} bytes) exceeds HEADER_SIZE ({})",
        serialized.len(),
        HEADER_SIZE
    );
    let mut buf = vec![0u8; HEADER_SIZE];
    buf[..serialized.len()].copy_from_slice(&serialized);
    let mut cursor = Cursor::new(buf);
    Header::read_from(&mut cursor).expect("read_from failed on valid header")
}

// ── read_from roundtrips ──────────────────────────────────────────────────────

#[test]
fn test_read_from_default() {
    let original = Header::default();
    let decoded = roundtrip(&original);
    assert_eq!(original, decoded);
}

#[test]
fn test_read_from_lz4_compression() {
    let mut h = Header::default();
    h.compression = CompressionType::Lz4;
    let decoded = roundtrip(&h);
    assert_eq!(decoded.compression, CompressionType::Lz4);
}

#[test]
fn test_read_from_zstd_compression() {
    let mut h = Header::default();
    h.compression = CompressionType::Zstd;
    let decoded = roundtrip(&h);
    assert_eq!(decoded.compression, CompressionType::Zstd);
}

#[test]
fn test_read_from_custom_block_size() {
    let mut h = Header::default();
    h.block_size = 131_072;
    let decoded = roundtrip(&h);
    assert_eq!(decoded.block_size, 131_072);
}

#[test]
fn test_read_from_with_parent_path() {
    let mut h = Header::default();
    h.parent_paths = vec!["/snapshots/base.hxz".to_string()];
    let decoded = roundtrip(&h);
    assert_eq!(decoded.parent_paths, vec!["/snapshots/base.hxz"]);
}

#[test]
fn test_read_from_with_multiple_parent_paths() {
    let mut h = Header::default();
    h.parent_paths = vec!["a.hxz".to_string(), "b.hxz".to_string()];
    let decoded = roundtrip(&h);
    assert_eq!(decoded.parent_paths.len(), 2);
    assert_eq!(decoded.parent_paths[0], "a.hxz");
    assert_eq!(decoded.parent_paths[1], "b.hxz");
}

#[test]
fn test_read_from_with_metadata_location() {
    let mut h = Header::default();
    h.metadata_offset = Some(1_048_576);
    h.metadata_length = Some(4096);
    let decoded = roundtrip(&h);
    assert_eq!(decoded.metadata_offset, Some(1_048_576));
    assert_eq!(decoded.metadata_length, Some(4096));
}

#[test]
fn test_read_from_with_signature_location() {
    let mut h = Header::default();
    h.signature_offset = Some(2_000_000);
    h.signature_length = Some(64);
    let decoded = roundtrip(&h);
    assert_eq!(decoded.signature_offset, Some(2_000_000));
    assert_eq!(decoded.signature_length, Some(64));
}

#[test]
fn test_read_from_feature_flags() {
    let mut h = Header::default();
    h.features = FeatureFlags {
        has_disk: true,
        has_memory: true,
        variable_blocks: false,
    };
    let decoded = roundtrip(&h);
    assert!(decoded.features.has_disk);
    assert!(decoded.features.has_memory);
    assert!(!decoded.features.variable_blocks);
}

#[test]
fn test_read_from_variable_blocks_flag() {
    let mut h = Header::default();
    h.features.variable_blocks = true;
    let decoded = roundtrip(&h);
    assert!(decoded.features.variable_blocks);
}

#[test]
fn test_read_from_magic_bytes_preserved() {
    let h = Header::default();
    let decoded = roundtrip(&h);
    assert_eq!(decoded.magic, *MAGIC_BYTES);
}

#[test]
fn test_read_from_version_preserved() {
    let h = Header::default();
    let decoded = roundtrip(&h);
    assert!(decoded.version > 0);
}

#[test]
fn test_read_from_index_offset_preserved() {
    let mut h = Header::default();
    h.index_offset = 4096;
    let decoded = roundtrip(&h);
    assert_eq!(decoded.index_offset, 4096);
}

// ── error paths ───────────────────────────────────────────────────────────────

#[test]
fn test_read_from_empty_input_fails() {
    let mut cursor = Cursor::new(vec![]);
    assert!(
        Header::read_from(&mut cursor).is_err(),
        "reading from empty input should fail"
    );
}

#[test]
fn test_read_from_truncated_input_fails() {
    // Only 512 bytes — too short for a valid HEADER_SIZE-byte read.
    let short = vec![0u8; 512];
    let mut cursor = Cursor::new(short);
    assert!(
        Header::read_from(&mut cursor).is_err(),
        "reading truncated input should fail"
    );
}

#[test]
fn test_serialized_size_fits_in_header() {
    // Ensures a fully-populated header still fits in the fixed HEADER_SIZE slot.
    let h = Header {
        magic: *MAGIC_BYTES,
        version: 1,
        block_size: 65536,
        index_offset: 999_999_999,
        parent_paths: vec!["a/very/long/parent/path/to/a/snapshot.hxz".to_string()],
        dictionary_offset: Some(1_234_567),
        dictionary_length: Some(32768),
        metadata_offset: Some(9_999_999),
        metadata_length: Some(8192),
        signature_offset: Some(10_000_000),
        signature_length: Some(64),
        encryption: None,
        compression: CompressionType::Zstd,
        features: FeatureFlags {
            has_disk: true,
            has_memory: true,
            variable_blocks: true,
        },
    };
    let bytes = bincode::serialize(&h).unwrap();
    assert!(
        bytes.len() <= HEADER_SIZE,
        "header serialization ({} bytes) must fit in HEADER_SIZE ({} bytes)",
        bytes.len(),
        HEADER_SIZE
    );
}
