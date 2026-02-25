//! Snapshot file header and related enums.

use hexz_common::constants::DEFAULT_BLOCK_SIZE;
use hexz_common::crypto::KeyDerivationParams;
use serde::{Deserialize, Serialize};

use super::magic::{FORMAT_VERSION, MAGIC_BYTES};

/// On-disk snapshot file header containing format metadata.
///
/// This structure is serialized at the beginning of every `.hxz` file and
/// describes the format version, compression settings, encryption parameters,
/// and locations of key data structures within the file.
///
/// # Binary Layout
///
/// The header occupies exactly 4096 bytes (HEADER_SIZE) at file offset 0 with
/// the following logical structure:
/// - Magic bytes (4): File signature "HEXZ"
/// - Version (4): Format version number
/// - Block size (4): Logical block size in bytes
/// - Index offset (8): File offset to the master index structure
/// - Parent path (variable): Optional path for thin snapshots
/// - Dictionary offset/length: Optional compression dictionary location
/// - Metadata offset/length: Optional user metadata location
/// - Signature offset/length: Optional cryptographic signature location
/// - Encryption parameters: Optional key derivation settings
/// - Compression type: Algorithm used (LZ4 or Zstd)
/// - Feature flags: Capabilities enabled in this snapshot
///
/// # Versioning
///
/// The version field enables forward compatibility. Readers check this field
/// and reject files with incompatible versions. The current format version is
/// defined in the `magic` module.
///
/// # Thin Provisioning
///
/// When `parent_paths` is set, this snapshot is a thin snapshot that references
/// blocks from the parent. Blocks marked with [`BLOCK_OFFSET_PARENT`] are
/// read from the parent snapshot instead of the current file.
///
/// [`BLOCK_OFFSET_PARENT`]: hexz_common::constants::BLOCK_OFFSET_PARENT
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Header {
    pub magic: [u8; 4],
    pub version: u32,
    pub block_size: u32,
    pub index_offset: u64,

    pub parent_paths: Vec<String>,

    pub dictionary_offset: Option<u64>,
    pub dictionary_length: Option<u32>,
    pub metadata_offset: Option<u64>,
    pub metadata_length: Option<u32>,
    pub signature_offset: Option<u64>,
    pub signature_length: Option<u32>,
    pub encryption: Option<KeyDerivationParams>,
    pub compression: CompressionType,
    pub features: FeatureFlags,
}

/// Compression algorithm used for block data.
///
/// This enum specifies which compression algorithm was used to compress
/// the data blocks stored in the snapshot file. The compressor must be
/// configured appropriately when reading the file.
///
/// # Supported Algorithms
///
/// - **LZ4**: Fast compression with lower ratios, ideal for latency-sensitive workloads
/// - **Zstd**: Balanced compression with optional dictionary training for higher ratios
///
/// # Performance Characteristics
///
/// - LZ4: ~500 MB/s compression, ~2000 MB/s decompression (single-threaded)
/// - Zstd (level 3): ~200 MB/s compression, ~600 MB/s decompression (single-threaded)
///
/// The actual performance depends on data characteristics, CPU capabilities, and
/// whether dictionary compression is enabled for Zstd.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressionType {
    /// LZ4 compression algorithm (fast, lower ratio)
    Lz4,
    /// Zstandard compression algorithm (balanced, supports dictionaries)
    Zstd,
}

/// Feature flags indicating capabilities enabled in this snapshot.
///
/// These boolean flags describe which optional features are present in the
/// snapshot file. Readers must check these flags to determine how to
/// interpret the file contents.
///
/// # Fields
///
/// - `has_disk`: Snapshot contains disk state (primary stream present in index)
/// - `has_memory`: Snapshot contains memory state (secondary stream present in index)
/// - `variable_blocks`: Content-defined chunking (CDC) was used instead of fixed-size blocks
///
/// # Usage
///
/// When both `has_disk` and `has_memory` are true, the snapshot is a full VM
/// snapshot that can be used for live migration or checkpoint/restore. When
/// only `has_disk` is true, it's a disk-only snapshot suitable for boot or backup.
///
/// The `variable_blocks` flag indicates that block sizes vary (CDC mode) and
/// readers must use the `logical_len` field from each [`BlockInfo`] rather than
/// assuming a fixed block size.
///
/// [`BlockInfo`]: super::index::BlockInfo
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FeatureFlags {
    /// Snapshot contains disk state
    pub has_disk: bool,
    /// Snapshot contains memory state
    pub has_memory: bool,
    /// Content-defined chunking (CDC) was used for variable-sized blocks
    pub variable_blocks: bool,
}

impl Header {
    /// Read and deserialize a header from a [`std::io::Read`] source.
    pub fn read_from<R: std::io::Read>(reader: &mut R) -> hexz_common::Result<Self> {
        let mut header_bytes = [0u8; super::magic::HEADER_SIZE];
        reader.read_exact(&mut header_bytes)?;
        let header: Header = bincode::deserialize(&header_bytes)?;
        Ok(header)
    }

    /// Read a header from a [`StorageBackend`](crate::store::StorageBackend) at offset 0.
    pub fn read_from_backend(
        backend: &dyn crate::store::StorageBackend,
    ) -> hexz_common::Result<Self> {
        let header_bytes = backend.read_exact(0, super::magic::HEADER_SIZE)?;
        let header: Header = bincode::deserialize(&header_bytes)?;
        Ok(header)
    }

    /// Load the compression dictionary from the backend, if present.
    pub fn load_dictionary(
        &self,
        backend: &dyn crate::store::StorageBackend,
    ) -> hexz_common::Result<Option<Vec<u8>>> {
        if let (Some(offset), Some(length)) = (self.dictionary_offset, self.dictionary_length) {
            Ok(Some(backend.read_exact(offset, length as usize)?.to_vec()))
        } else {
            Ok(None)
        }
    }
}

impl Default for Header {
    fn default() -> Self {
        Self {
            magic: *MAGIC_BYTES,
            version: FORMAT_VERSION,
            block_size: DEFAULT_BLOCK_SIZE,
            index_offset: 0,
            parent_paths: Vec::new(),
            dictionary_offset: None,
            dictionary_length: None,
            metadata_offset: None,
            metadata_length: None,
            signature_offset: None,
            signature_length: None,
            encryption: None,
            compression: CompressionType::Lz4,
            features: FeatureFlags::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hexz_header_default() {
        let header = Header::default();

        assert_eq!(header.magic, *MAGIC_BYTES);
        assert_eq!(header.version, FORMAT_VERSION);
        assert_eq!(header.block_size, DEFAULT_BLOCK_SIZE);
        assert_eq!(header.index_offset, 0);
        assert!(header.parent_paths.is_empty());
        assert!(header.dictionary_offset.is_none());
        assert!(header.dictionary_length.is_none());
        assert!(header.metadata_offset.is_none());
        assert!(header.metadata_length.is_none());
        assert!(header.signature_offset.is_none());
        assert!(header.signature_length.is_none());
        assert!(header.encryption.is_none());
        assert_eq!(header.compression, CompressionType::Lz4);
        assert!(!header.features.has_disk);
        assert!(!header.features.has_memory);
        assert!(!header.features.variable_blocks);
    }

    #[test]
    fn test_hexz_header_with_disk() {
        let mut header = Header::default();
        header.features.has_disk = true;
        header.index_offset = 1048576;

        assert!(header.features.has_disk);
        assert_eq!(header.index_offset, 1048576);
    }

    #[test]
    fn test_hexz_header_with_memory() {
        let mut header = Header::default();
        header.features.has_memory = true;

        assert!(header.features.has_memory);
    }

    #[test]
    fn test_hexz_header_with_both_streams() {
        let mut header = Header::default();
        header.features.has_disk = true;
        header.features.has_memory = true;

        assert!(header.features.has_disk);
        assert!(header.features.has_memory);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_hexz_header_with_parent_path() {
        let mut header = Header::default();
        header.parent_paths = vec!["/path/to/parent.hxz".to_string()];

        assert_eq!(header.parent_paths, vec!["/path/to/parent.hxz"]);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_hexz_header_with_dictionary() {
        let mut header = Header::default();
        header.dictionary_offset = Some(4096);
        header.dictionary_length = Some(16384);

        assert_eq!(header.dictionary_offset, Some(4096));
        assert_eq!(header.dictionary_length, Some(16384));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_hexz_header_with_metadata() {
        let mut header = Header::default();
        header.metadata_offset = Some(20480);
        header.metadata_length = Some(1024);

        assert_eq!(header.metadata_offset, Some(20480));
        assert_eq!(header.metadata_length, Some(1024));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_hexz_header_with_signature() {
        let mut header = Header::default();
        header.signature_offset = Some(24576);
        header.signature_length = Some(256);

        assert_eq!(header.signature_offset, Some(24576));
        assert_eq!(header.signature_length, Some(256));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_hexz_header_with_encryption() {
        let mut header = Header::default();
        header.encryption = Some(KeyDerivationParams {
            salt: [0x42; 16],
            iterations: 100000,
        });

        assert!(header.encryption.is_some());
        let params = header.encryption.unwrap();
        assert_eq!(params.salt, [0x42; 16]);
        assert_eq!(params.iterations, 100000);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_hexz_header_zstd_compression() {
        let mut header = Header::default();
        header.compression = CompressionType::Zstd;

        assert_eq!(header.compression, CompressionType::Zstd);
    }

    #[test]
    fn test_hexz_header_variable_blocks() {
        let mut header = Header::default();
        header.features.variable_blocks = true;

        assert!(header.features.variable_blocks);
    }

    #[test]
    fn test_hexz_header_serialization() {
        let header = Header::default();

        let bytes = bincode::serialize(&header).unwrap();
        let deserialized: Header = bincode::deserialize(&bytes).unwrap();

        assert_eq!(deserialized, header);
    }

    #[test]
    fn test_hexz_header_serialization_with_all_fields() {
        let header = Header {
            magic: *MAGIC_BYTES,
            version: FORMAT_VERSION,
            block_size: 65536,
            index_offset: 1048576,
            parent_paths: vec!["/parent.hxz".to_string()],
            dictionary_offset: Some(4096),
            dictionary_length: Some(16384),
            metadata_offset: Some(20480),
            metadata_length: Some(1024),
            signature_offset: Some(24576),
            signature_length: Some(256),
            encryption: Some(KeyDerivationParams {
                salt: [0x42; 16],
                iterations: 100000,
            }),
            compression: CompressionType::Zstd,
            features: FeatureFlags {
                has_disk: true,
                has_memory: true,
                variable_blocks: true,
            },
        };

        let bytes = bincode::serialize(&header).unwrap();
        let deserialized: Header = bincode::deserialize(&bytes).unwrap();

        assert_eq!(deserialized, header);
        assert_eq!(deserialized.block_size, 65536);
        assert_eq!(deserialized.parent_paths, vec!["/parent.hxz"]);
        assert!(deserialized.features.has_disk);
        assert!(deserialized.features.has_memory);
        assert!(deserialized.features.variable_blocks);
    }

    #[test]
    fn test_hexz_header_equality() {
        let header1 = Header::default();
        let header2 = Header::default();

        assert_eq!(header1, header2);
    }

    #[test]
    fn test_hexz_header_inequality() {
        let mut header1 = Header::default();
        let mut header2 = Header::default();

        header1.block_size = 4096;
        header2.block_size = 65536;

        assert_ne!(header1, header2);
    }

    #[test]
    fn test_compression_type_lz4() {
        let compression = CompressionType::Lz4;
        assert_eq!(compression, CompressionType::Lz4);
    }

    #[test]
    fn test_compression_type_zstd() {
        let compression = CompressionType::Zstd;
        assert_eq!(compression, CompressionType::Zstd);
    }

    #[test]
    fn test_compression_type_equality() {
        assert_eq!(CompressionType::Lz4, CompressionType::Lz4);
        assert_eq!(CompressionType::Zstd, CompressionType::Zstd);
        assert_ne!(CompressionType::Lz4, CompressionType::Zstd);
    }

    #[test]
    fn test_compression_type_serialization() {
        let lz4 = CompressionType::Lz4;
        let bytes = bincode::serialize(&lz4).unwrap();
        let deserialized: CompressionType = bincode::deserialize(&bytes).unwrap();
        assert_eq!(deserialized, CompressionType::Lz4);

        let zstd = CompressionType::Zstd;
        let bytes = bincode::serialize(&zstd).unwrap();
        let deserialized: CompressionType = bincode::deserialize(&bytes).unwrap();
        assert_eq!(deserialized, CompressionType::Zstd);
    }

    #[test]
    fn test_feature_flags_default() {
        let flags = FeatureFlags::default();

        assert!(!flags.has_disk);
        assert!(!flags.has_memory);
        assert!(!flags.variable_blocks);
    }

    #[test]
    fn test_feature_flags_disk_only() {
        let flags = FeatureFlags {
            has_disk: true,
            has_memory: false,
            variable_blocks: false,
        };

        assert!(flags.has_disk);
        assert!(!flags.has_memory);
        assert!(!flags.variable_blocks);
    }

    #[test]
    fn test_feature_flags_memory_only() {
        let flags = FeatureFlags {
            has_disk: false,
            has_memory: true,
            variable_blocks: false,
        };

        assert!(!flags.has_disk);
        assert!(flags.has_memory);
        assert!(!flags.variable_blocks);
    }

    #[test]
    fn test_feature_flags_full_vm_snapshot() {
        let flags = FeatureFlags {
            has_disk: true,
            has_memory: true,
            variable_blocks: false,
        };

        assert!(flags.has_disk);
        assert!(flags.has_memory);
        assert!(!flags.variable_blocks);
    }

    #[test]
    fn test_feature_flags_with_variable_blocks() {
        let flags = FeatureFlags {
            has_disk: true,
            has_memory: false,
            variable_blocks: true,
        };

        assert!(flags.has_disk);
        assert!(!flags.has_memory);
        assert!(flags.variable_blocks);
    }

    #[test]
    fn test_feature_flags_all_enabled() {
        let flags = FeatureFlags {
            has_disk: true,
            has_memory: true,
            variable_blocks: true,
        };

        assert!(flags.has_disk);
        assert!(flags.has_memory);
        assert!(flags.variable_blocks);
    }

    #[test]
    fn test_feature_flags_equality() {
        let flags1 = FeatureFlags {
            has_disk: true,
            has_memory: false,
            variable_blocks: false,
        };

        let flags2 = FeatureFlags {
            has_disk: true,
            has_memory: false,
            variable_blocks: false,
        };

        assert_eq!(flags1, flags2);
    }

    #[test]
    fn test_feature_flags_inequality() {
        let flags1 = FeatureFlags {
            has_disk: true,
            has_memory: false,
            variable_blocks: false,
        };

        let flags2 = FeatureFlags {
            has_disk: false,
            has_memory: true,
            variable_blocks: false,
        };

        assert_ne!(flags1, flags2);
    }

    #[test]
    fn test_feature_flags_serialization() {
        let flags = FeatureFlags {
            has_disk: true,
            has_memory: true,
            variable_blocks: false,
        };

        let bytes = bincode::serialize(&flags).unwrap();
        let deserialized: FeatureFlags = bincode::deserialize(&bytes).unwrap();

        assert_eq!(deserialized, flags);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_hexz_header_custom_block_size() {
        let mut header = Header::default();
        header.block_size = 131072; // 128 KB

        assert_eq!(header.block_size, 131072);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_hexz_header_large_index_offset() {
        let mut header = Header::default();
        header.index_offset = 1099511627776; // 1 TB

        assert_eq!(header.index_offset, 1099511627776);
    }

    #[test]
    fn test_hexz_header_clone() {
        let header1 = Header::default();
        let header2 = header1.clone();

        assert_eq!(header1, header2);
    }

    #[test]
    fn test_feature_flags_clone() {
        let flags1 = FeatureFlags {
            has_disk: true,
            has_memory: true,
            variable_blocks: true,
        };
        let flags2 = flags1;

        assert_eq!(flags1, flags2);
    }

    #[test]
    fn test_compression_type_clone() {
        let comp1 = CompressionType::Zstd;
        let comp2 = comp1;

        assert_eq!(comp1, comp2);
    }

    #[test]
    fn test_hexz_header_debug_format() {
        let header = Header::default();
        let debug_str = format!("{:?}", header);

        assert!(debug_str.contains("Header"));
        assert!(debug_str.contains("magic"));
        assert!(debug_str.contains("version"));
    }
}
