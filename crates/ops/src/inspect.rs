//! Archive inspection and metadata extraction.

use hexz_common::Result;
use hexz_core::format::header::{CompressionType, Header};
use hexz_core::format::index::{IndexPage, MasterIndex};
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Per-block statistics for an archive.
#[derive(Debug, Default, serde::Serialize)]
pub struct BlockStats {
    /// Number of data blocks.
    pub data_blocks: usize,
    /// Total uncompressed bytes in data blocks.
    pub data_bytes: u64,
    /// Number of parent-reference blocks.
    pub parent_ref_blocks: usize,
    /// Total bytes represented by parent references.
    pub parent_ref_bytes: u64,
    /// Number of zero (sparse) blocks.
    pub zero_blocks: usize,
    /// Total bytes represented by zero blocks.
    pub zero_bytes: u64,

    /// Smallest data block size in bytes.
    pub min_block_size: u32,
    /// Largest data block size in bytes.
    pub max_block_size: u32,
    /// Average data block size in bytes.
    pub avg_block_size: u32,

    /// Number of unique (non-duplicate) blocks.
    pub unique_blocks: usize,
    /// Number of deduplicated blocks.
    pub dedup_blocks: usize,
    /// Bytes saved by deduplication.
    pub dedup_bytes_saved: u64,

    /// Total compressed bytes for data blocks.
    pub compressed_data_bytes: u64,
}

/// Feature flags describing what an archive contains.
#[derive(Debug, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ArchiveFeatures {
    /// Whether the archive is encrypted.
    pub encrypted: bool,
    /// Whether a main stream is present.
    pub has_main: bool,
    /// Whether an auxiliary stream is present.
    pub has_auxiliary: bool,
    /// Whether variable-size blocks are enabled.
    pub variable_blocks: bool,
    /// Whether a cryptographic signature is present.
    pub signature_present: bool,
    /// Whether a compression dictionary is present.
    pub dictionary_present: bool,
}

/// Metadata extracted from an archive file.
pub struct ArchiveInfo {
    /// Archive format version.
    pub version: u32,
    /// Block size in bytes.
    pub block_size: u32,
    /// Compression algorithm used.
    pub compression: CompressionType,
    /// Paths to parent archives for delta dedup.
    pub parent_paths: Vec<String>,
    /// Feature flags for this archive.
    pub features: ArchiveFeatures,
    /// Total uncompressed size of the main stream.
    pub main_size: u64,
    /// Total uncompressed size of the auxiliary stream.
    pub auxiliary_size: u64,
    /// On-disk file size in bytes.
    pub file_size: u64,
    /// Byte offset of the master index.
    pub index_offset: u64,
    /// Number of index pages for the main stream.
    pub main_pages: usize,
    /// Number of index pages for the auxiliary stream.
    pub auxiliary_pages: usize,
    /// Byte offset of embedded metadata, if any.
    pub metadata_offset: Option<u64>,
    /// Length of embedded metadata in bytes.
    pub metadata_length: Option<u32>,
    /// Decoded metadata string, if present.
    pub metadata: Option<String>,
    /// Detailed block-level statistics.
    pub block_stats: Option<BlockStats>,
}

impl std::fmt::Debug for ArchiveInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArchiveInfo")
            .field("version", &self.version)
            .field("block_size", &self.block_size)
            .field("compression", &self.compression)
            .field("file_size", &self.file_size)
            .finish_non_exhaustive()
    }
}

impl ArchiveInfo {
    /// Returns the total uncompressed size (main + auxiliary).
    pub const fn total_uncompressed(&self) -> u64 {
        self.main_size + self.auxiliary_size
    }

    /// Returns the compression ratio (uncompressed / file size).
    pub fn compression_ratio(&self) -> f64 {
        if self.file_size > 0 {
            self.total_uncompressed() as f64 / self.file_size as f64
        } else {
            0.0
        }
    }
}

/// Reads and returns metadata and block statistics for an archive file.
pub fn inspect_archive(path: impl AsRef<Path>) -> Result<ArchiveInfo> {
    let mut f = File::open(path.as_ref())?;
    let file_size = f.metadata()?.len();

    let header = Header::read_from(&mut f)?;
    let master = MasterIndex::read_from(&mut f, header.index_offset)?;

    let metadata = if let (Some(off), Some(len)) = (header.metadata_offset, header.metadata_length)
    {
        let mut buf = vec![0u8; len as usize];
        _ = f.seek(SeekFrom::Start(off))?;
        f.read_exact(&mut buf)?;
        Some(String::from_utf8_lossy(&buf).to_string())
    } else {
        None
    };

    let mut stats = BlockStats {
        min_block_size: u32::MAX,
        ..Default::default()
    };
    let mut seen_offsets: HashSet<u64> = HashSet::new();

    for page_meta in &master.main_pages {
        _ = f.seek(SeekFrom::Start(page_meta.offset))?;
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
                stats.compressed_data_bytes += block.length as u64;

                if block.logical_len < stats.min_block_size {
                    stats.min_block_size = block.logical_len;
                }
                if block.logical_len > stats.max_block_size {
                    stats.max_block_size = block.logical_len;
                }

                if seen_offsets.insert(block.offset) {
                    stats.unique_blocks += 1;
                } else {
                    stats.dedup_blocks += 1;
                    stats.dedup_bytes_saved += block.logical_len as u64;
                }
            }
        }
    }

    if stats.data_blocks > 0 {
        stats.avg_block_size = (stats.data_bytes / stats.data_blocks as u64) as u32;
    } else {
        stats.min_block_size = 0;
    }

    Ok(ArchiveInfo {
        version: header.version,
        block_size: header.block_size,
        compression: header.compression,
        parent_paths: header.parent_paths,
        features: ArchiveFeatures {
            encrypted: header.encryption.is_some(),
            has_main: header.features.has_main,
            has_auxiliary: header.features.has_auxiliary,
            variable_blocks: header.features.variable_blocks,
            signature_present: header.signature_offset.is_some(),
            dictionary_present: header.dictionary_offset.is_some(),
        },
        main_size: master.main_size,
        auxiliary_size: master.auxiliary_size,
        file_size,
        index_offset: header.index_offset,
        main_pages: master.main_pages.len(),
        auxiliary_pages: master.auxiliary_pages.len(),
        metadata_offset: header.metadata_offset,
        metadata_length: header.metadata_length,
        metadata,
        block_stats: Some(stats),
    })
}
