//! Archive file header and related enums.

use hexz_common::constants::DEFAULT_BLOCK_SIZE;
use hexz_common::crypto::KeyDerivationParams;
use serde::{Deserialize, Serialize};

use super::magic::{FORMAT_VERSION, MAGIC_BYTES};

/// On-disk archive file header containing format metadata.
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

    /// Content-defined chunking parameters used for this archive.
    /// (fingerprint_bits, min_chunk, max_chunk)
    pub cdc_params: Option<(u32, u32, u32)>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressionType {
    /// LZ4 compression algorithm (fast, lower ratio)
    Lz4,
    /// Zstandard compression algorithm (balanced, supports dictionaries)
    Zstd,
}

/// Feature flags indicating capabilities enabled in this archive.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FeatureFlags {
    /// Archive contains a main data stream
    pub has_main: bool,
    /// Archive contains an auxiliary data stream
    pub has_auxiliary: bool,
    /// Content-defined chunking (CDC) was used for variable-sized blocks
    pub variable_blocks: bool,
}

impl Header {
    /// Read and deserialize a header from a [`std::io::Read`] source.
    pub fn read_from<R: std::io::Read>(reader: &mut R) -> hexz_common::Result<Self> {
        let mut header_bytes = [0u8; super::magic::HEADER_SIZE];
        reader.read_exact(&mut header_bytes)?;
        let header: Header = bincode::deserialize(&header_bytes)?;
        if &header.magic != MAGIC_BYTES {
            return Err(hexz_common::Error::Format("Invalid magic bytes".into()));
        }
        Ok(header)
    }

    /// Read a header from a [`StorageBackend`](crate::store::StorageBackend) at offset 0.
    pub fn read_from_backend(
        backend: &dyn crate::store::StorageBackend,
    ) -> hexz_common::Result<Self> {
        let header_bytes = backend.read_exact(0, super::magic::HEADER_SIZE)?;
        let header: Header = bincode::deserialize(&header_bytes)?;
        if &header.magic != MAGIC_BYTES {
            return Err(hexz_common::Error::Format("Invalid magic bytes".into()));
        }
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
            cdc_params: None,
        }
    }
}
