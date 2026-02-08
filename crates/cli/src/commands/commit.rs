//! Commit overlay and optional memory into a new snapshot.

use anyhow::Result;
use indicatif::ProgressBar;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use strata_common::constants::{BLOCK_OFFSET_PARENT, DEFAULT_ZSTD_LEVEL};
use strata_core::StrataFile;
use strata_core::algo::compression::{Compressor, lz4::Lz4Compressor, zstd::ZstdCompressor};
use strata_core::api::stratafile::SnapshotStream;
use strata_core::format::{
    header::{CompressionType, FeatureFlags, StrataHeader},
    index::{BlockInfo, ENTRIES_PER_PAGE, IndexPage, MasterIndex, PageEntry},
    magic::{FORMAT_VERSION, HEADER_SIZE, MAGIC_BYTES},
};
use strata_core::store::StorageBackend;
use strata_core::store::local::FileBackend;

const OVERLAY_BLOCK_SIZE: u64 = 4096;
const META_ENTRY_SIZE: usize = 8;

#[allow(clippy::too_many_arguments)]
pub fn run(
    base_path: PathBuf,
    overlay_path: PathBuf,
    memory_path: Option<PathBuf>,
    output_path: PathBuf,
    algo: String,
    block_size: u32,
    keep_overlay: bool,
    message: Option<String>,
    thin: bool, // NEW FLAG
) -> Result<()> {
    println!(
        "Committing changes from {:?} to {:?} (Thin: {})",
        overlay_path, output_path, thin
    );

    let backend = Arc::new(FileBackend::new(&base_path)?);
    let header_bytes = backend.read_exact(0, HEADER_SIZE)?;
    let header: StrataHeader = bincode::deserialize(&header_bytes)?;

    let read_compressor: Box<dyn Compressor> = match header.compression {
        CompressionType::Lz4 => Box::new(Lz4Compressor::new()),
        CompressionType::Zstd => {
            let dict = if let (Some(off), Some(len)) =
                (header.dictionary_offset, header.dictionary_length)
            {
                Some(backend.read_exact(off, len as usize)?.to_vec())
            } else {
                None
            };
            Box::new(ZstdCompressor::new(DEFAULT_ZSTD_LEVEL, dict))
        }
    };

    let base_snap = Arc::new(StrataFile::new(backend, read_compressor, None)?);

    let meta_path = overlay_path.with_extension("meta");
    let mut modified_blocks = std::collections::HashSet::new();

    if meta_path.exists() {
        let mut f = File::open(&meta_path)?;
        let mut buf = [0u8; META_ENTRY_SIZE];
        loop {
            match f.read_exact(&mut buf) {
                Ok(_) => {
                    modified_blocks.insert(u64::from_le_bytes(buf));
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
        }
    }

    let mut overlay_file = File::open(&overlay_path)?;

    let mut out = File::create(&output_path)?;
    out.write_all(&[0u8; HEADER_SIZE])?;
    let mut current_offset = HEADER_SIZE;

    let write_compressor: Box<dyn Compressor> = match algo.as_str() {
        "zstd" => Box::new(ZstdCompressor::new(DEFAULT_ZSTD_LEVEL, None)),
        _ => Box::new(Lz4Compressor::new()),
    };

    let mut master = MasterIndex::default();
    let mut global_block_idx = 0;

    let base_disk_size = base_snap.size(SnapshotStream::Disk);
    let overlay_len = overlay_file.metadata()?.len();
    let final_disk_size = std::cmp::max(base_disk_size, overlay_len);
    master.disk_size = final_disk_size;

    let pb = ProgressBar::new(final_disk_size);
    let mut page = IndexPage::default();
    let mut page_start_block = 0;
    let mut page_start_logical = 0u64;
    let mut current_logical_pos = 0u64;

    let bs = block_size as u64;
    let total_blocks = final_disk_size.div_ceil(bs);

    for i in 0..total_blocks {
        let block_start = i * bs;
        let mut block_len = bs;
        if block_start + block_len > final_disk_size {
            block_len = final_disk_size - block_start;
        }

        let mut is_modified = false;
        let start_ov_blk = block_start / OVERLAY_BLOCK_SIZE;
        let end_ov_blk = (block_start + block_len - 1) / OVERLAY_BLOCK_SIZE;

        for ov_blk in start_ov_blk..=end_ov_blk {
            if modified_blocks.contains(&ov_blk) {
                is_modified = true;
                break;
            }
        }

        // THIN PROVISIONING LOGIC
        if thin && !is_modified && block_start < base_disk_size {
            // If thin mode, not modified, and exists in base:
            // Write a marker pointing to parent.
            page.blocks.push(BlockInfo {
                offset: BLOCK_OFFSET_PARENT,
                length: 0,
                logical_len: block_len as u32,
                checksum: 0,
            });

            global_block_idx += 1;
            current_logical_pos += block_len;
            pb.inc(block_len);

            if page.blocks.len() >= ENTRIES_PER_PAGE {
                flush_page(
                    &mut page,
                    &mut out,
                    &mut current_offset,
                    &mut master.disk_pages,
                    page_start_block,
                    page_start_logical,
                )?;
                page_start_block = global_block_idx;
                page_start_logical = current_logical_pos;
            }
            continue;
        }

        // Standard Logic (Thick or Modified)
        let mut data = vec![0u8; block_len as usize];

        if is_modified {
            // Read base first for partial updates
            if block_start < base_disk_size {
                let base_data =
                    base_snap.read_at(SnapshotStream::Disk, block_start, block_len as usize)?;
                data[..base_data.len()].copy_from_slice(&base_data);
            }

            // Apply overlay
            for ov_blk in start_ov_blk..=end_ov_blk {
                if modified_blocks.contains(&ov_blk) {
                    let chunk_start = ov_blk * OVERLAY_BLOCK_SIZE;
                    let chunk_end =
                        std::cmp::min(chunk_start + OVERLAY_BLOCK_SIZE, final_disk_size);
                    let chunk_len = (chunk_end - chunk_start) as usize;
                    let rel_start = (chunk_start - block_start) as usize;

                    overlay_file.seek(SeekFrom::Start(chunk_start))?;
                    let _ = overlay_file.read(&mut data[rel_start..rel_start + chunk_len])?;
                }
            }
        } else if block_start < base_disk_size {
            // Not modified, but thick mode: Copy from base
            let base_data =
                base_snap.read_at(SnapshotStream::Disk, block_start, block_len as usize)?;
            data[..base_data.len()].copy_from_slice(&base_data);
        }

        if data.iter().all(|&b| b == 0) {
            page.blocks.push(BlockInfo {
                offset: 0,
                length: 0,
                logical_len: block_len as u32,
                checksum: 0,
            });
        } else {
            let compressed = write_compressor.compress(&data)?;
            let checksum = crc32fast::hash(&compressed);

            out.write_all(&compressed)?;
            page.blocks.push(BlockInfo {
                offset: current_offset as u64,
                length: compressed.len() as u32,
                logical_len: block_len as u32,
                checksum,
            });
            current_offset += compressed.len();
        }

        global_block_idx += 1;
        current_logical_pos += block_len;
        pb.inc(block_len);

        if page.blocks.len() >= ENTRIES_PER_PAGE {
            flush_page(
                &mut page,
                &mut out,
                &mut current_offset,
                &mut master.disk_pages,
                page_start_block,
                page_start_logical,
            )?;
            page_start_block = global_block_idx;
            page_start_logical = current_logical_pos;
        }
    }

    if !page.blocks.is_empty() {
        flush_page(
            &mut page,
            &mut out,
            &mut current_offset,
            &mut master.disk_pages,
            page_start_block,
            page_start_logical,
        )?;
    }

    // Handle Memory (Always thick for now, as memory changes drastically)
    if let Some(mem_path) = memory_path {
        println!("\nProcessing new memory state...");
        let mut mem_file = File::open(&mem_path)?;
        let mem_len = mem_file.metadata()?.len();
        master.memory_size = mem_len;

        let pb_mem = ProgressBar::new(mem_len);
        let mut page = IndexPage::default();
        let mut page_start_block = global_block_idx;
        let mut page_start_logical = 0u64;
        let mut current_mem_logical = 0u64;

        let mut buf = vec![0u8; block_size as usize];

        loop {
            let n = mem_file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            let chunk = &buf[..n];

            if chunk.iter().all(|&b| b == 0) {
                page.blocks.push(BlockInfo {
                    offset: 0,
                    length: 0,
                    logical_len: n as u32,
                    checksum: 0,
                });
            } else {
                let compressed = write_compressor.compress(chunk)?;
                let checksum = crc32fast::hash(&compressed);

                out.write_all(&compressed)?;
                page.blocks.push(BlockInfo {
                    offset: current_offset as u64,
                    length: compressed.len() as u32,
                    logical_len: n as u32,
                    checksum,
                });
                current_offset += compressed.len();
            }

            global_block_idx += 1;
            current_mem_logical += n as u64;
            pb_mem.inc(n as u64);

            if page.blocks.len() >= ENTRIES_PER_PAGE {
                flush_page(
                    &mut page,
                    &mut out,
                    &mut current_offset,
                    &mut master.memory_pages,
                    page_start_block,
                    page_start_logical,
                )?;
                page_start_block = global_block_idx;
                page_start_logical = current_mem_logical;
            }
        }

        if !page.blocks.is_empty() {
            flush_page(
                &mut page,
                &mut out,
                &mut current_offset,
                &mut master.memory_pages,
                page_start_block,
                page_start_logical,
            )?;
        }
    } else {
        master.memory_size = 0;
        master.memory_pages.clear();
    }

    let index_offset = current_offset as u64;
    let index_bytes = bincode::serialize(&master)?;
    out.write_all(&index_bytes)?;

    let (metadata_offset, metadata_length) = if let Some(msg) = message {
        let msg_bytes = msg.as_bytes();
        let off = current_offset as u64 + index_bytes.len() as u64;
        out.write_all(msg_bytes)?;
        (Some(off), Some(msg_bytes.len() as u32))
    } else {
        (None, None)
    };

    // Determine parent path for header
    let parent_path = if thin {
        // Store absolute path to base for v1 simplicity
        Some(
            std::fs::canonicalize(&base_path)?
                .to_string_lossy()
                .to_string(),
        )
    } else {
        None
    };

    let new_header = StrataHeader {
        magic: *MAGIC_BYTES,
        version: FORMAT_VERSION,
        block_size,
        index_offset,
        parent_path, // Set parent path
        dictionary_offset: None,
        dictionary_length: None,
        metadata_offset,
        metadata_length,
        signature_offset: None,
        signature_length: None,
        encryption: None,
        compression: if algo == "zstd" {
            CompressionType::Zstd
        } else {
            CompressionType::Lz4
        },
        features: FeatureFlags {
            has_disk: !master.disk_pages.is_empty(),
            has_memory: !master.memory_pages.is_empty(),
            variable_blocks: false,
        },
    };

    out.seek(SeekFrom::Start(0))?;
    out.write_all(&bincode::serialize(&new_header)?)?;

    if !keep_overlay {
        println!("Cleaning up overlay files...");
        let _ = std::fs::remove_file(&overlay_path);
        let _ = std::fs::remove_file(&meta_path);
    }

    println!("Commit complete: {:?}", output_path);
    Ok(())
}

fn flush_page(
    page: &mut IndexPage,
    out: &mut File,
    current_offset: &mut usize,
    pages_vec: &mut Vec<PageEntry>,
    start_block: u64,
    start_logical: u64,
) -> Result<()> {
    let bytes = bincode::serialize(&page)?;
    let p_off = *current_offset;
    out.write_all(&bytes)?;
    *current_offset += bytes.len();
    pages_vec.push(PageEntry {
        offset: p_off as u64,
        length: bytes.len() as u32,
        start_block,
        start_logical,
    });
    *page = IndexPage::default();
    Ok(())
}
