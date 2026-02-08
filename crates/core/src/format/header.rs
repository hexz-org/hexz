//! Snapshot file header and related enums.

use serde::{Deserialize, Serialize};
use strata_common::constants::DEFAULT_BLOCK_SIZE;
use strata_common::crypto::KeyDerivationParams;

use super::magic::{FORMAT_VERSION, MAGIC_BYTES};

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressionType {
    Lz4,
    Zstd,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FeatureFlags {
    pub has_disk: bool,
    pub has_memory: bool,
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
