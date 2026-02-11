//! Snapshot file header and related enums.

use serde::{Deserialize, Serialize};
use strata_common::constants::DEFAULT_BLOCK_SIZE;
use strata_common::crypto::KeyDerivationParams;

use super::magic::{FORMAT_VERSION, MAGIC_BYTES};

/// On-disk snapshot file header containing format metadata.
///
/// This structure is serialized at the beginning of every `.st` file and
/// describes the format version, compression settings, encryption parameters,
/// and locations of key data structures within the file.
///
/// # Binary Layout
///
/// The header occupies exactly 4096 bytes (HEADER_SIZE) at file offset 0 with
/// the following logical structure:
/// - Magic bytes (4): File signature "STRT"
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
/// defined in [`super::magic::FORMAT_VERSION`].
///
/// # Thin Provisioning
///
/// When `parent_path` is set, this snapshot is a thin snapshot that references
/// blocks from the parent. Blocks marked with [`BLOCK_OFFSET_PARENT`] are
/// read from the parent snapshot instead of the current file.
///
/// [`BLOCK_OFFSET_PARENT`]: strata_common::constants::BLOCK_OFFSET_PARENT
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StrataHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub block_size: u32,
    pub index_offset: u64,

    /// Path to the parent snapshot for thin provisioning.
    /// If None, this is a standalone (thick) snapshot.
    pub parent_path: Option<String>,

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
/// - `has_disk`: Snapshot contains disk state (disk stream present in index)
/// - `has_memory`: Snapshot contains memory state (memory stream present in index)
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

impl Default for StrataHeader {
    fn default() -> Self {
        Self {
            magic: *MAGIC_BYTES,
            version: FORMAT_VERSION,
            block_size: DEFAULT_BLOCK_SIZE,
            index_offset: 0,
            parent_path: None,
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
