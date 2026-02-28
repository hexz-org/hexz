//! Compare block hashes between two Hexz archives.
//!
//! Reports how much data is shared between two snapshots, how many blocks are
//! unique to each, and the implied storage savings from deduplication.
//!
//! # Common Usage
//!
//! ```bash
//! hexz diff base.hxz finetuned.hxz
//! ```

use anyhow::{Context, Result};
use hexz_core::format::header::Header;
use hexz_core::format::index::{IndexPage, MasterIndex};
use hexz_ops::inspect::inspect_snapshot;
use indicatif::HumanBytes;
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::ui::color::palette;

/// Per-block classification for one archive, derived from a single index scan.
struct BlockSummary {
    /// Hashes of blocks with actual data stored in this file.
    hashes: HashSet<[u8; 32]>,
    /// Bytes covered by parent-ref blocks (shared with parent by definition).
    parent_ref_bytes: u64,
    /// Number of parent-ref blocks.
    parent_ref_blocks: usize,
    /// Bytes of data blocks whose hash is not in `hashes` of the other file.
    unique_bytes: u64,
    unique_blocks: usize,
}

fn scan(path: &Path) -> Result<BlockSummary> {
    let mut f = File::open(path)?;
    let header = Header::read_from(&mut f)?;
    let master = MasterIndex::read_from(&mut f, header.index_offset)?;

    let mut hashes = HashSet::new();
    let mut parent_ref_bytes = 0u64;
    let mut parent_ref_blocks = 0usize;

    for page_meta in &master.primary_pages {
        f.seek(SeekFrom::Start(page_meta.offset))?;
        let mut buf = vec![0u8; page_meta.length as usize];
        f.read_exact(&mut buf)?;
        let page: IndexPage = bincode::deserialize(&buf)?;
        for block in page.blocks {
            if block.is_parent_ref() {
                parent_ref_blocks += 1;
                parent_ref_bytes += block.logical_len as u64;
            } else if !block.is_sparse() && block.hash != [0u8; 32] {
                hashes.insert(block.hash);
            }
        }
    }

    Ok(BlockSummary {
        hashes,
        parent_ref_bytes,
        parent_ref_blocks,
        unique_bytes: 0,
        unique_blocks: 0,
    })
}

/// Compare two archives and report shared vs. unique block data.
pub fn run(a: PathBuf, b: PathBuf) -> Result<()> {
    let info_a = inspect_snapshot(&a).with_context(|| format!("Failed to read {}", a.display()))?;
    let info_b = inspect_snapshot(&b).with_context(|| format!("Failed to read {}", b.display()))?;

    let mut summary_a =
        scan(&a).with_context(|| format!("Failed to read blocks from {}", a.display()))?;
    let mut summary_b =
        scan(&b).with_context(|| format!("Failed to read blocks from {}", b.display()))?;

    // Classify each file's data blocks as shared or unique relative to the other.
    // parent-ref blocks in B are shared with A by definition (they point at the parent).
    let mut shared_blocks = summary_b.parent_ref_blocks;
    let mut shared_bytes = summary_b.parent_ref_bytes;

    // Scan B's data blocks against A's hash set.
    {
        let mut f = File::open(&b)?;
        let header = Header::read_from(&mut f)?;
        let master = MasterIndex::read_from(&mut f, header.index_offset)?;

        for page_meta in &master.primary_pages {
            f.seek(SeekFrom::Start(page_meta.offset))?;
            let mut buf = vec![0u8; page_meta.length as usize];
            f.read_exact(&mut buf)?;
            let page: IndexPage = bincode::deserialize(&buf)?;
            for block in page.blocks {
                if block.is_parent_ref() || block.is_sparse() || block.hash == [0u8; 32] {
                    continue;
                }
                if summary_a.hashes.contains(&block.hash) {
                    shared_blocks += 1;
                    shared_bytes += block.logical_len as u64;
                } else {
                    summary_b.unique_blocks += 1;
                    summary_b.unique_bytes += block.logical_len as u64;
                }
            }
        }
    }

    // Scan A's data blocks against B's hash set for unique-to-A count.
    {
        let mut f = File::open(&a)?;
        let header = Header::read_from(&mut f)?;
        let master = MasterIndex::read_from(&mut f, header.index_offset)?;

        for page_meta in &master.primary_pages {
            f.seek(SeekFrom::Start(page_meta.offset))?;
            let mut buf = vec![0u8; page_meta.length as usize];
            f.read_exact(&mut buf)?;
            let page: IndexPage = bincode::deserialize(&buf)?;
            for block in page.blocks {
                if block.is_parent_ref() || block.is_sparse() || block.hash == [0u8; 32] {
                    continue;
                }
                if !summary_b.hashes.contains(&block.hash) {
                    summary_a.unique_blocks += 1;
                    summary_a.unique_bytes += block.logical_len as u64;
                }
            }
        }
    }

    // --- Render ---
    let p = palette();

    let name_a = a.file_name().unwrap_or(a.as_os_str()).to_string_lossy();
    let name_b = b.file_name().unwrap_or(b.as_os_str()).to_string_lossy();
    let max_name = name_a.len().max(name_b.len());

    let total_a_data_blocks = summary_a.hashes.len();
    let total_b_data_blocks = summary_b.hashes.len() + summary_b.parent_ref_blocks;

    // Pre-format all alignment-sensitive strings as plain text so ANSI codes
    // don't skew column widths.
    let name_a_col = format!("{:<w$}", name_a, w = max_name);
    let name_b_col = format!("{:<w$}", name_b, w = max_name);
    let size_a_col = format!("{:>10}", HumanBytes(info_a.file_size));
    let size_b_col = format!("{:>10}", HumanBytes(info_b.file_size));
    let blk_a_col = format!("{:>6}", total_a_data_blocks);
    let blk_b_col = format!("{:>6}", total_b_data_blocks);

    // Label column width: wide enough for "Only in <longest_name>:"
    let lbl_w = "Only in ".len() + max_name + 1;
    let shared_lbl = format!("{:<w$}", "Shared:", w = lbl_w);
    let new_b_lbl = format!("{:<w$}", format!("New in {}:", name_b), w = lbl_w);
    let only_a_lbl = format!("{:<w$}", format!("Only in {}:", name_a), w = lbl_w);
    let saved_lbl = format!("{:<w$}", "Storage saved:", w = lbl_w);

    let shared_size = format!("{:>10}", HumanBytes(shared_bytes));
    let new_b_size = format!("{:>10}", HumanBytes(summary_b.unique_bytes));
    let only_a_size = format!("{:>10}", HumanBytes(summary_a.unique_bytes));
    let shared_blk = format!("{:>6}", shared_blocks);
    let new_b_blk = format!("{:>6}", summary_b.unique_blocks);
    let only_a_blk = format!("{:>6}", summary_a.unique_blocks);

    let total_b_bytes = (shared_bytes + summary_b.unique_bytes).max(1);
    let pct = |n: u64| n as f64 / total_b_bytes as f64 * 100.0;

    let is_thin_b = summary_b.parent_ref_blocks > 0;
    let thin_note = if is_thin_b {
        format!(
            "  {}({} via parent refs){}",
            p.gray, summary_b.parent_ref_blocks, p.reset
        )
    } else {
        String::new()
    };

    // File header
    println!();
    println!(
        "  {}{}{}  {}{}{}  {} blocks",
        p.bold, name_a_col, p.reset, p.green, size_a_col, p.reset, blk_a_col
    );
    println!(
        "  {}{}{}  {}{}{}  {} blocks",
        p.bold, name_b_col, p.reset, p.green, size_b_col, p.reset, blk_b_col
    );
    println!();

    // Comparison rows
    println!(
        "  {}{}{}  {}{}{}  {} blocks  {}({:.0}%){}{}",
        p.cyan,
        shared_lbl,
        p.reset,
        p.green,
        shared_size,
        p.reset,
        shared_blk,
        p.bold,
        pct(shared_bytes),
        p.reset,
        thin_note,
    );
    println!(
        "  {}{}{}  {}{}{}  {} blocks",
        p.cyan, new_b_lbl, p.reset, p.yellow, new_b_size, p.reset, new_b_blk,
    );
    if !is_thin_b {
        println!(
            "  {}{}{}  {}{}{}  {} blocks",
            p.cyan, only_a_lbl, p.reset, p.dim, only_a_size, p.reset, only_a_blk,
        );
    }
    println!();
    println!(
        "  {}{}{}  {}{}{}",
        p.cyan, saved_lbl, p.reset, p.green, shared_size, p.reset
    );
    println!();

    Ok(())
}
