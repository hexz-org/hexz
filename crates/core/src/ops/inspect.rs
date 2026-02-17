//! Snapshot inspection without fully opening the file.

use crate::format::header::{CompressionType, Header};
use crate::format::index::MasterIndex;
use hexz_common::Result;
use std::fs::File;
use std::path::Path;

/// Metadata extracted from a snapshot file.
pub struct SnapshotInfo {
    pub version: u32,
    pub block_size: u32,
    pub compression: CompressionType,
    pub encrypted: bool,
    pub parent_path: Option<String>,
    pub has_disk: bool,
    pub has_memory: bool,
    pub variable_blocks: bool,
    pub primary_size: u64,
    pub secondary_size: u64,
    pub file_size: u64,
    pub index_offset: u64,
    pub primary_pages: usize,
    pub secondary_pages: usize,
    pub signature_present: bool,
    pub dictionary_present: bool,
    pub metadata_offset: Option<u64>,
    pub metadata_length: Option<u32>,
}

impl SnapshotInfo {
    /// Total uncompressed size (disk + memory).
    pub fn total_uncompressed(&self) -> u64 {
        self.primary_size + self.secondary_size
    }

    /// Compression ratio (uncompressed / file size).
    pub fn compression_ratio(&self) -> f64 {
        if self.file_size > 0 {
            self.total_uncompressed() as f64 / self.file_size as f64
        } else {
            0.0
        }
    }
}

/// Inspect a snapshot and extract metadata without fully opening it.
pub fn inspect_snapshot(path: impl AsRef<Path>) -> Result<SnapshotInfo> {
    let mut f = File::open(path.as_ref())?;
    let file_size = f.metadata()?.len();

    let header = Header::read_from(&mut f)?;
    let master = MasterIndex::read_from(&mut f, header.index_offset)?;

    Ok(SnapshotInfo {
        version: header.version,
        block_size: header.block_size,
        compression: header.compression,
        encrypted: header.encryption.is_some(),
        parent_path: header.parent_path,
        has_disk: header.features.has_disk,
        has_memory: header.features.has_memory,
        variable_blocks: header.features.variable_blocks,
        primary_size: master.primary_size,
        secondary_size: master.secondary_size,
        file_size,
        index_offset: header.index_offset,
        primary_pages: master.primary_pages.len(),
        secondary_pages: master.secondary_pages.len(),
        signature_present: header.signature_offset.is_some(),
        dictionary_present: header.dictionary_offset.is_some(),
        metadata_offset: header.metadata_offset,
        metadata_length: header.metadata_length,
    })
}
