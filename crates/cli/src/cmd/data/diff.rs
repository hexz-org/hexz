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
    let name_a = a.file_name().unwrap_or(a.as_os_str()).to_string_lossy();
    let name_b = b.file_name().unwrap_or(b.as_os_str()).to_string_lossy();
    let max_name = name_a.len().max(name_b.len());

    let total_a_data_blocks = summary_a.hashes.len();
    let total_b_data_blocks = summary_b.hashes.len() + summary_b.parent_ref_blocks;

    println!();
    println!(
        "  {:<width$}  {:>10}  {:>6} blocks",
        name_a,
        HumanBytes(info_a.file_size),
        total_a_data_blocks,
        width = max_name,
    );
    println!(
        "  {:<width$}  {:>10}  {:>6} blocks",
        name_b,
        HumanBytes(info_b.file_size),
        total_b_data_blocks,
        width = max_name,
    );
    println!();

    let total_b_bytes = (shared_bytes + summary_b.unique_bytes).max(1);
    let pct = |n: u64| n as f64 / total_b_bytes as f64 * 100.0;

    // When B is a thin snapshot, its parent-ref blocks cover data owned by A.
    // Those hashes aren't stored in B's index, so A's blocks appear "not found in B"
    // even though they're shared. Suppress the misleading "only in A" count in that case.
    let is_thin_b = summary_b.parent_ref_blocks > 0;
    let thin_note = if is_thin_b {
        format!("  ({} via parent refs)", summary_b.parent_ref_blocks)
    } else {
        String::new()
    };

    println!(
        "  Shared:        {:>10}  {:>6} blocks  ({:.0}%){}",
        HumanBytes(shared_bytes),
        shared_blocks,
        pct(shared_bytes),
        thin_note,
    );
    println!(
        "  New in {:<width$}  {:>10}  {:>6} blocks",
        format!("{}:", name_b),
        HumanBytes(summary_b.unique_bytes),
        summary_b.unique_blocks,
        width = max_name + 1,
    );
    if !is_thin_b {
        println!(
            "  Only in {:<width$}  {:>10}  {:>6} blocks",
            format!("{}:", name_a),
            HumanBytes(summary_a.unique_bytes),
            summary_a.unique_blocks,
            width = max_name + 1,
        );
    }
    println!();
    println!("  Storage saved: {}", HumanBytes(shared_bytes));
    println!();

    Ok(())
}
