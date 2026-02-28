//! Compare block hashes between two Hexz archives.
//!
//! Reports how much data is shared between two snapshots at the block-hash
//! level, and — when the archives contain checkpoint manifests — at the
//! logical checkpoint-delta level (XOR delta tensors).
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
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::ui::color::palette;

/// Per-block classification for one archive, derived from a single index scan.
struct BlockSummary {
    /// Unique data block hashes → uncompressed logical length.
    ///
    /// Using a map (not a set) lets us compute byte totals from set operations
    /// without re-reading the index.
    data: HashMap<[u8; 32], u64>,
    /// Bytes covered by parent-ref blocks (logically shared with the parent).
    parent_ref_bytes: u64,
    /// Number of parent-ref block entries.
    parent_ref_blocks: usize,
}

fn scan(path: &Path) -> Result<BlockSummary> {
    let mut f = File::open(path)?;
    let header = Header::read_from(&mut f)?;
    let master = MasterIndex::read_from(&mut f, header.index_offset)?;

    let mut data: HashMap<[u8; 32], u64> = HashMap::new();
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
                // or_insert keeps the first logical_len seen for a given hash;
                // blocks with the same hash always have the same content/size.
                data.entry(block.hash).or_insert(block.logical_len as u64);
            }
        }
    }

    Ok(BlockSummary {
        data,
        parent_ref_bytes,
        parent_ref_blocks,
    })
}

/// XOR-delta checkpoint statistics parsed from archive B's manifest.
struct CheckpointDelta {
    /// Number of tensors using XOR delta encoding.
    xor_delta_count: usize,
    /// Sum of `base_length` across all XOR delta tensors (uncompressed base in A).
    xor_base_bytes: u64,
    /// True when B's declared parent matches A by filename.
    parent_is_a: bool,
    /// Filename of B's declared parent (for display).
    parent_name: String,
}

/// Parse checkpoint manifest from B to extract XOR delta tensor statistics.
///
/// Returns `None` if the metadata is not a checkpoint, or if no tensors use
/// XOR delta encoding.
fn parse_checkpoint_delta(
    meta_b: &str,
    name_a: &str,
    parent_paths_b: &[String],
) -> Option<CheckpointDelta> {
    let obj: serde_json::Value = serde_json::from_str(meta_b).ok()?;
    obj.get("hexz_checkpoint")?; // must be a checkpoint manifest
    let tensors = obj.get("tensors")?.as_object()?;

    let mut xor_delta_count = 0usize;
    let mut xor_base_bytes = 0u64;

    for (_, tensor) in tensors {
        let storage = tensor
            .get("storage")
            .and_then(|v| v.as_str())
            .unwrap_or("raw");
        let base_length = tensor
            .get("base_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        if storage == "xor_delta" {
            xor_delta_count += 1;
            xor_base_bytes += base_length;
        }
    }

    if xor_delta_count == 0 {
        return None;
    }

    let parent_name = parent_paths_b
        .first()
        .and_then(|p| Path::new(p).file_name())
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parent_is_a = parent_name == name_a;

    Some(CheckpointDelta {
        xor_delta_count,
        xor_base_bytes,
        parent_is_a,
        parent_name,
    })
}

/// Compare two archives and report shared vs. unique block data.
pub fn run(a: PathBuf, b: PathBuf) -> Result<()> {
    let info_a = inspect_snapshot(&a).with_context(|| format!("Failed to read {}", a.display()))?;
    let info_b = inspect_snapshot(&b).with_context(|| format!("Failed to read {}", b.display()))?;

    let summary_a =
        scan(&a).with_context(|| format!("Failed to read blocks from {}", a.display()))?;
    let summary_b =
        scan(&b).with_context(|| format!("Failed to read blocks from {}", b.display()))?;

    // Set operations on unique hashes — consistent with the header counts.
    // parent-ref blocks in B are counted as "shared" (they point at A's data).
    let shared_data_blocks: usize = summary_b
        .data
        .keys()
        .filter(|h| summary_a.data.contains_key(*h))
        .count();
    let shared_data_bytes: u64 = summary_b
        .data
        .iter()
        .filter(|(h, _)| summary_a.data.contains_key(*h))
        .map(|(_, &len)| len)
        .sum();

    let shared_blocks = shared_data_blocks + summary_b.parent_ref_blocks;
    let shared_bytes = shared_data_bytes + summary_b.parent_ref_bytes;

    let new_b_blocks: usize = summary_b
        .data
        .keys()
        .filter(|h| !summary_a.data.contains_key(*h))
        .count();
    let new_b_bytes: u64 = summary_b
        .data
        .iter()
        .filter(|(h, _)| !summary_a.data.contains_key(*h))
        .map(|(_, &len)| len)
        .sum();

    let only_a_blocks: usize = summary_a
        .data
        .keys()
        .filter(|h| !summary_b.data.contains_key(*h))
        .count();
    let only_a_bytes: u64 = summary_a
        .data
        .iter()
        .filter(|(h, _)| !summary_b.data.contains_key(*h))
        .map(|(_, &len)| len)
        .sum();

    // Optional: checkpoint delta stats from B's manifest.
    let name_a_str = a
        .file_name()
        .unwrap_or(a.as_os_str())
        .to_string_lossy()
        .into_owned();
    let name_b_str = b
        .file_name()
        .unwrap_or(b.as_os_str())
        .to_string_lossy()
        .into_owned();

    let cp_delta = info_b
        .metadata
        .as_deref()
        .and_then(|m| parse_checkpoint_delta(m, &name_a_str, &info_b.parent_paths));

    // --- Render ---
    let p = palette();

    let max_name = name_a_str.len().max(name_b_str.len());

    // Pre-format all alignment-sensitive strings as plain text so ANSI codes
    // don't skew column widths.
    let name_a_col = format!("{:<w$}", name_a_str, w = max_name);
    let name_b_col = format!("{:<w$}", name_b_str, w = max_name);
    let size_a_col = format!("{:>10}", HumanBytes(info_a.file_size));
    let size_b_col = format!("{:>10}", HumanBytes(info_b.file_size));
    let blk_a_col = format!("{:>6}", summary_a.data.len() + summary_a.parent_ref_blocks);
    let blk_b_col = format!("{:>6}", summary_b.data.len() + summary_b.parent_ref_blocks);

    // Label column: wide enough for "Only in <longest_name>:"
    let lbl_w = "Only in ".len() + max_name + 1;
    let shared_lbl = format!("{:<w$}", "Shared:", w = lbl_w);
    let new_b_lbl = format!("{:<w$}", format!("New in {}:", name_b_str), w = lbl_w);
    let only_a_lbl = format!("{:<w$}", format!("Only in {}:", name_a_str), w = lbl_w);
    let shared_size_col = format!("{:>10}", HumanBytes(shared_bytes));
    let new_b_size_col = format!("{:>10}", HumanBytes(new_b_bytes));
    let only_a_size_col = format!("{:>10}", HumanBytes(only_a_bytes));
    let shared_blk_col = format!("{:>6}", shared_blocks);
    let new_b_blk_col = format!("{:>6}", new_b_blocks);
    let only_a_blk_col = format!("{:>6}", only_a_blocks);

    let total_b_bytes = (shared_bytes + new_b_bytes).max(1);
    let pct = |n: u64| n as f64 / total_b_bytes as f64 * 100.0;

    let is_thin_b = summary_b.parent_ref_blocks > 0;
    let is_xor_delta = cp_delta.is_some();

    let thin_note = if is_thin_b {
        format!(
            "  {}({} via parent refs){}",
            p.gray, summary_b.parent_ref_blocks, p.reset
        )
    } else {
        String::new()
    };

    // "Storage saved" means different things depending on archive type:
    //   • plain / thin: bytes already in A that B doesn't need to re-store
    //   • XOR delta: how much smaller B is on disk vs A (proxy for delta compression saving)
    let (saved_label, saved_bytes) = if is_xor_delta {
        (
            "Delta saving:",
            info_a.file_size.saturating_sub(info_b.file_size),
        )
    } else {
        ("Storage saved:", shared_bytes)
    };
    let saved_lbl = format!("{:<w$}", saved_label, w = lbl_w);
    let saved_size_col = format!("{:>10}", HumanBytes(saved_bytes));

    // B header tag.
    let b_delta_tag = if is_xor_delta {
        format!("  {}(XOR delta checkpoint){}", p.dim, p.reset)
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
        "  {}{}{}  {}{}{}  {} blocks{}",
        p.bold, name_b_col, p.reset, p.green, size_b_col, p.reset, blk_b_col, b_delta_tag
    );
    println!();

    if is_xor_delta {
        // For XOR delta archives the block-hash comparison is always 0% (XOR produces
        // unique hashes) and always shows the same logical sizes (XOR preserves tensor
        // size). Suppress those rows — they would be identical noise for every step.
        // The checkpoint delta section below carries all the meaningful information.
    } else {
        // Block-level comparison rows (plain / thin archives only).
        println!(
            "  {}{}{}  {}{}{}  {} blocks  {}({:.0}%){}{}",
            p.cyan,
            shared_lbl,
            p.reset,
            p.green,
            shared_size_col,
            p.reset,
            shared_blk_col,
            p.bold,
            pct(shared_bytes),
            p.reset,
            thin_note,
        );
        println!(
            "  {}{}{}  {}{}{}  {} blocks",
            p.cyan, new_b_lbl, p.reset, p.yellow, new_b_size_col, p.reset, new_b_blk_col,
        );
        if !is_thin_b {
            println!(
                "  {}{}{}  {}{}{}  {} blocks",
                p.cyan, only_a_lbl, p.reset, p.dim, only_a_size_col, p.reset, only_a_blk_col,
            );
        }
        println!();
    }

    // Checkpoint delta section (only when B has XOR-delta tensors).
    if let Some(ref d) = cp_delta {
        let base_name = if d.parent_is_a {
            &name_a_str
        } else {
            &d.parent_name
        };
        let base_tag = if d.parent_is_a {
            format!("{}{}{}", p.yellow, base_name, p.reset)
        } else {
            // B derives from someone else, not A — flag it clearly.
            format!(
                "{}{}{} {}(not {}){}",
                p.yellow, base_name, p.reset, p.gray, name_a_str, p.reset
            )
        };

        let compression_ratio = d.xor_base_bytes as f64 / info_b.file_size as f64;

        println!(
            "  {}Checkpoint delta{}  ({} tensors use XOR delta off {})",
            p.bold, p.reset, d.xor_delta_count, base_tag,
        );
        println!(
            "    {}{}{} base  →  {}{}{} on disk  {}({:.1}×  compression){}",
            p.green,
            HumanBytes(d.xor_base_bytes),
            p.reset,
            p.yellow,
            HumanBytes(info_b.file_size),
            p.reset,
            p.dim,
            compression_ratio,
            p.reset,
        );
        println!(
            "    {}{}{} {}required for reconstruction{}",
            p.yellow, base_name, p.reset, p.dim, p.reset,
        );
        println!();
    }

    println!(
        "  {}{}{}  {}{}{}",
        p.cyan, saved_lbl, p.reset, p.green, saved_size_col, p.reset
    );
    println!();

    Ok(())
}
