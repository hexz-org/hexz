//! Archive inspection and metadata extraction.

use hexz_common::Result;
use hexz_core::format::header::{CompressionType, Header};
use hexz_core::format::index::{IndexPage, MasterIndex};
use std::collections::HashSet;
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

    pub min_block_size: u32,
    pub max_block_size: u32,
    pub avg_block_size: u32,

    pub unique_blocks: usize,
    pub dedup_blocks: usize,
    pub dedup_bytes_saved: u64,

    pub compressed_data_bytes: u64,
}

/// Metadata extracted from a archive file.
pub struct ArchiveInfo {
    pub version: u32,
    pub block_size: u32,
    pub compression: CompressionType,
    pub encrypted: bool,
    pub parent_paths: Vec<String>,
    pub has_main: bool,
    pub has_auxiliary: bool,
    pub variable_blocks: bool,
    pub main_size: u64,
    pub auxiliary_size: u64,
    pub file_size: u64,
    pub index_offset: u64,
    pub main_pages: usize,
    pub auxiliary_pages: usize,
    pub signature_present: bool,
    pub dictionary_present: bool,
    pub metadata_offset: Option<u64>,
    pub metadata_length: Option<u32>,
    pub metadata: Option<String>,
    pub block_stats: Option<BlockStats>,
}

impl ArchiveInfo {
    pub fn total_uncompressed(&self) -> u64 {
        self.main_size + self.auxiliary_size
    }

    pub fn compression_ratio(&self) -> f64 {
        if self.file_size > 0 {
            self.total_uncompressed() as f64 / self.file_size as f64
        } else {
            0.0
        }
    }
}

pub fn inspect_archive(path: impl AsRef<Path>) -> Result<ArchiveInfo> {
    let mut f = File::open(path.as_ref())?;
    let file_size = f.metadata()?.len();

    let header = Header::read_from(&mut f)?;
    let master = MasterIndex::read_from(&mut f, header.index_offset)?;

    let metadata = if let (Some(off), Some(len)) = (header.metadata_offset, header.metadata_length)
    {
        let mut buf = vec![0u8; len as usize];
        f.seek(SeekFrom::Start(off))?;
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
        encrypted: header.encryption.is_some(),
        parent_paths: header.parent_paths,
        has_main: header.features.has_main,
        has_auxiliary: header.features.has_auxiliary,
        variable_blocks: header.features.variable_blocks,
        main_size: master.main_size,
        auxiliary_size: master.auxiliary_size,
        file_size,
        index_offset: header.index_offset,
        main_pages: master.main_pages.len(),
        auxiliary_pages: master.auxiliary_pages.len(),
        signature_present: header.signature_offset.is_some(),
        dictionary_present: header.dictionary_offset.is_some(),
        metadata_offset: header.metadata_offset,
        metadata_length: header.metadata_length,
        metadata,
        block_stats: Some(stats),
    })
}
