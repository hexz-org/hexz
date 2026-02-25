//! Snapshot inspection and metadata extraction.

use hexz_common::Result;
use hexz_core::format::header::{CompressionType, Header};
use hexz_core::format::index::{IndexPage, MasterIndex};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Default, serde::Serialize)]
pub struct BlockStats {
    pub data_blocks: usize,
    pub data_bytes: u64,
    pub parent_ref_blocks: usize,
    pub parent_ref_bytes: u64,
    pub zero_blocks: usize,
    pub zero_bytes: u64,
}

/// Metadata extracted from a snapshot file.
pub struct SnapshotInfo {
    pub version: u32,
    pub block_size: u32,
    pub compression: CompressionType,
    pub encrypted: bool,
    pub parent_paths: Vec<String>,
    pub has_primary: bool,
    pub has_secondary: bool,
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
    pub metadata: Option<String>,
    pub block_stats: Option<BlockStats>,
}

impl SnapshotInfo {
    /// Total uncompressed size (primary + secondary).
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

/// Inspect a snapshot and extract all metadata, including block-level deduplication stats.
pub fn inspect_snapshot(path: impl AsRef<Path>) -> Result<SnapshotInfo> {
    let mut f = File::open(path.as_ref())?;
    let file_size = f.metadata()?.len();

    let header = Header::read_from(&mut f)?;
    let master = MasterIndex::read_from(&mut f, header.index_offset)?;

    // 1. Extract Custom Metadata (e.g., file manifests, commit messages)
    let metadata = if let (Some(off), Some(len)) = (header.metadata_offset, header.metadata_length)
    {
        let mut buf = vec![0u8; len as usize];
        f.seek(SeekFrom::Start(off))?;
        f.read_exact(&mut buf)?;

        // Convert to UTF-8 string, ignoring invalid sequences
        Some(String::from_utf8_lossy(&buf).to_string())
    } else {
        None
    };

    // 2. Tally Deduplication Block Stats for the primary stream
    let mut stats = BlockStats::default();

    for page_meta in &master.primary_pages {
        f.seek(SeekFrom::Start(page_meta.offset))?;
        let mut page_bytes = vec![0u8; page_meta.length as usize];
        f.read_exact(&mut page_bytes)?;

        let page: IndexPage = bincode::deserialize(&page_bytes)?;

        for block in page.blocks {
            if block.is_parent_ref() {
                stats.parent_ref_blocks += 1;
                stats.parent_ref_bytes += block.logical_len as u64;
            } else if block.is_sparse() {
                stats.zero_blocks += 1;
                stats.zero_bytes += block.logical_len as u64;
            } else {
                stats.data_blocks += 1;
                stats.data_bytes += block.logical_len as u64;
            }
        }
    }

    Ok(SnapshotInfo {
        version: header.version,
        block_size: header.block_size,
        compression: header.compression,
        encrypted: header.encryption.is_some(),
        parent_paths: header.parent_paths,
        has_primary: header.features.has_disk,
        has_secondary: header.features.has_memory,
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
        metadata,
        block_stats: Some(stats),
    })
}
