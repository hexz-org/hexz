//! List Hexz archives in a directory and render their lineage as a tree.
//!
//! Scans a directory for `.hxz` files, reads the `parent_paths` field from
//! each header, and builds a parent→child graph.  Files whose parent lives
//! outside the scanned directory are shown with an external-parent annotation.
//!
//! # Common Usage
//!
//! ```bash
//! hexz ls ./checkpoints/
//! hexz ls .
//! ```
//!
//! # Output Example
//!
//! ```text
//! ./checkpoints/
//! ├── base.hxz                 12.4 GB  standalone
//! │   ├── epoch1.hxz            1.2 GB  +162 new blocks
//! │   │   └── epoch2.hxz        0.8 GB  +97 new blocks
//! │   └── finetune-v1.hxz       2.1 GB  +389 new blocks
//! └── unrelated.hxz             4.0 GB  standalone
//!
//! 5 archives   20.5 GB on disk
//! ```

use anyhow::{Context, Result};
use hexz_core::format::header::Header;
use hexz_core::format::index::MasterIndex;
use indicatif::HumanBytes;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::{Path, PathBuf};

use crate::ui::color::{Palette, palette};

struct ArchiveInfo {
    path: PathBuf,
    file_size: u64,
    /// First declared parent path (if any).
    parent: Option<String>,
    /// Number of data blocks (not parent-refs, not sparse).
    data_blocks: usize,
    /// Human-readable message from checkpoint metadata.
    message: Option<String>,
    /// Pre-formatted scalar summary like "step=8, loss=2.2702".
    scalars_summary: Option<String>,
    /// Number of tensors stored as XOR deltas (0 = not a delta checkpoint).
    xor_delta_count: usize,
    /// Sum of `base_length` for XOR delta tensors (uncompressed base bytes in parent).
    xor_base_bytes: u64,
}

fn read_archive_info(path: &Path) -> Result<ArchiveInfo> {
    use hexz_core::format::index::IndexPage;
    use std::io::{Read, Seek, SeekFrom};

    let mut f = File::open(path)?;
    let file_size = f.metadata()?.len();
    let header = Header::read_from(&mut f)?;
    let master = MasterIndex::read_from(&mut f, header.index_offset)?;

    let parent = header.parent_paths.into_iter().next();

    // Count data blocks (non-sparse, non-parent-ref) in primary stream.
    let mut data_blocks = 0usize;
    for page_meta in &master.primary_pages {
        f.seek(SeekFrom::Start(page_meta.offset))?;
        let mut buf = vec![0u8; page_meta.length as usize];
        f.read_exact(&mut buf)?;
        let page: IndexPage = bincode::deserialize(&buf)?;
        for block in page.blocks {
            if !block.is_sparse() && !block.is_parent_ref() {
                data_blocks += 1;
            }
        }
    }

    // Read metadata JSON for message / scalars / XOR delta info.
    let mut message = None;
    let mut scalars_summary = None;
    let mut xor_delta_count = 0usize;
    let mut xor_base_bytes = 0u64;
    if let (Some(meta_off), Some(meta_len)) = (header.metadata_offset, header.metadata_length) {
        if meta_len > 0 {
            f.seek(SeekFrom::Start(meta_off))?;
            let mut meta_buf = vec![0u8; meta_len as usize];
            f.read_exact(&mut meta_buf)?;
            if let Ok(obj) = serde_json::from_slice::<serde_json::Value>(&meta_buf) {
                message = obj
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let Some(scalars) = obj.get("scalars").and_then(|v| v.as_object()) {
                    if !scalars.is_empty() {
                        scalars_summary = Some(super::inspect::format_scalars_summary(scalars));
                    }
                }
                // Detect XOR delta checkpoint tensors.
                if let Some(tensors) = obj.get("tensors").and_then(|v| v.as_object()) {
                    for (_, tensor) in tensors {
                        let storage = tensor
                            .get("storage")
                            .and_then(|v| v.as_str())
                            .unwrap_or("raw");
                        if storage == "xor_delta" {
                            xor_delta_count += 1;
                            xor_base_bytes += tensor
                                .get("base_length")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                        }
                    }
                }
            }
        }
    }

    Ok(ArchiveInfo {
        path: path.to_path_buf(),
        file_size,
        parent,
        data_blocks,
        message,
        scalars_summary,
        xor_delta_count,
        xor_base_bytes,
    })
}

fn print_tree(
    idx: usize,
    entries: &[ArchiveInfo],
    children: &HashMap<usize, Vec<usize>>,
    external_parent: &[Option<&str>],
    prefix: &str,
    is_last: bool,
    p: &'static Palette,
) {
    let a = &entries[idx];
    let connector = if is_last { "└──" } else { "├──" };
    let name = a.path.file_name().unwrap_or_default().to_string_lossy();

    // Build the annotation text.  XOR delta archives replace "+N new blocks"
    // with a compression ratio; message / scalars are surfaced on top of that.
    let xor_tag = if a.xor_delta_count > 0 {
        let ratio = a.xor_base_bytes as f64 / a.file_size.max(1) as f64;
        format!(
            "  {}[XOR δ  {}t  {:.1}×]{}",
            p.dim, a.xor_delta_count, ratio, p.reset
        )
    } else {
        String::new()
    };

    let (ann_text, ann_color) = if let Some(ext) = external_parent[idx] {
        let parent_name = Path::new(ext)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        (format!("← {} (external)", parent_name), p.gray)
    } else if let Some(msg) = &a.message {
        (format!("{}{}", msg, xor_tag), p.yellow)
    } else if let Some(scalars) = &a.scalars_summary {
        (format!("{}{}", scalars, xor_tag), p.dim)
    } else if a.parent.is_none() {
        ("standalone".to_string(), p.gray)
    } else if a.xor_delta_count > 0 {
        // No message/scalars but IS a delta checkpoint — just show the ratio.
        let ratio = a.xor_base_bytes as f64 / a.file_size.max(1) as f64;
        (
            format!("XOR delta  {} tensors  {:.1}×", a.xor_delta_count, ratio),
            p.yellow,
        )
    } else {
        (format!("+{} new blocks", a.data_blocks), p.dim)
    };

    // Pre-pad with plain strings so ANSI codes don't skew column widths.
    let name_padded = format!("{:<32}", name);
    let size_str = format!("{:>10}", HumanBytes(a.file_size));

    println!(
        "  {}{}{}{} {}{}{} {}{}{}  {}{}{}",
        prefix,
        p.gray,
        connector,
        p.reset,
        p.bold,
        name_padded,
        p.reset,
        p.green,
        size_str,
        p.reset,
        ann_color,
        ann_text,
        p.reset,
    );

    let mut kids = children.get(&idx).cloned().unwrap_or_default();
    kids.sort_by_key(|&i| &entries[i].path);

    // Continue the vertical bar in gray, or use blank space for the last child.
    let segment = if is_last {
        "    ".to_string()
    } else {
        format!("{}│{}   ", p.gray, p.reset)
    };
    let child_prefix = format!("{}{}", prefix, segment);

    for (j, &child) in kids.iter().enumerate() {
        let last = j == kids.len() - 1;
        print_tree(
            child,
            entries,
            children,
            external_parent,
            &child_prefix,
            last,
            p,
        );
    }
}

/// Print a tree of all `.hxz` archives found in `dir`.
pub fn run(dir: PathBuf) -> Result<()> {
    // --- Collect all .hxz files ---
    let entries: Vec<ArchiveInfo> = std::fs::read_dir(&dir)
        .with_context(|| format!("Cannot read directory: {}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "hxz"))
        .map(|e| {
            let p = e.path();
            read_archive_info(&p).with_context(|| format!("Failed to read {}", p.display()))
        })
        .collect::<Result<Vec<_>>>()?;

    if entries.is_empty() {
        println!("No .hxz archives found in {}", dir.display());
        return Ok(());
    }

    // --- Build name → index map (using just the filename for matching) ---
    // parent_paths are stored as full paths at write time; match on filename.
    let name_to_idx: HashMap<String, usize> = entries
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let name = a
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            (name, i)
        })
        .collect();

    // For each archive, resolve its parent to a local index (if present).
    let parent_idx: Vec<Option<usize>> = entries
        .iter()
        .map(|a| {
            a.parent.as_deref().and_then(|p| {
                let parent_name = Path::new(p)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                name_to_idx.get(&parent_name).copied()
            })
        })
        .collect();

    // Archives whose parent is outside the scanned dir (declared but unresolved).
    let external_parent: Vec<Option<&str>> = entries
        .iter()
        .zip(&parent_idx)
        .map(|(a, resolved)| {
            if resolved.is_none() {
                a.parent.as_deref()
            } else {
                None
            }
        })
        .collect();

    // Build children map: parent_idx → Vec<child_idx>
    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut has_parent: HashSet<usize> = HashSet::new();
    for (i, p) in parent_idx.iter().enumerate() {
        if let Some(pi) = p {
            children.entry(*pi).or_default().push(i);
            has_parent.insert(i);
        }
    }

    // Roots = archives with no resolved local parent (including external-parent ones).
    let mut roots: Vec<usize> = (0..entries.len())
        .filter(|i| !has_parent.contains(i))
        .collect();
    roots.sort_by_key(|&i| &entries[i].path);

    // --- Render ---
    let p = palette();
    let total_size: u64 = entries.iter().map(|a| a.file_size).sum();

    // Strip trailing slash so we don't print "dir//" when the user passes "dir/".
    let dir_str = dir.to_string_lossy();
    let dir_base = dir_str.trim_end_matches('/');
    println!("\n  {}{}/{}", p.bold, dir_base, p.reset);

    for (i, &root) in roots.iter().enumerate() {
        let last = i == roots.len() - 1;
        print_tree(root, &entries, &children, &external_parent, "", last, p);
    }

    println!();
    println!(
        "  {}{}{} archive{}   {}{}{} on disk",
        p.bold,
        entries.len(),
        p.reset,
        if entries.len() == 1 { "" } else { "s" },
        p.green,
        HumanBytes(total_size),
        p.reset,
    );

    // If any archives are XOR delta checkpoints, show chain storage context.
    let delta_count = entries.iter().filter(|a| a.xor_delta_count > 0).count();
    if delta_count > 0 {
        // Total uncompressed base bytes referenced by all delta archives.
        let total_base: u64 = entries.iter().map(|a| a.xor_base_bytes).sum();
        // Standalone size: rough estimate — assume each delta would be as large as
        // the largest non-delta archive in the set (the base checkpoint).
        let base_size = entries
            .iter()
            .filter(|a| a.xor_delta_count == 0 && a.parent.is_none())
            .map(|a| a.file_size)
            .max()
            .unwrap_or(0);
        let standalone_estimate = base_size * delta_count as u64;
        let saved = standalone_estimate.saturating_sub(
            entries
                .iter()
                .filter(|a| a.xor_delta_count > 0)
                .map(|a| a.file_size)
                .sum::<u64>(),
        );
        println!(
            "  {}{} XOR delta checkpoint{}{}  ({}{}{} base data referenced)",
            p.dim,
            delta_count,
            if delta_count == 1 { "" } else { "s" },
            p.reset,
            p.green,
            HumanBytes(total_base),
            p.reset,
        );
        if saved > 0 && standalone_estimate > 0 {
            println!(
                "  {}~{} saved vs {} standalone copies{}",
                p.dim,
                HumanBytes(saved),
                delta_count,
                p.reset,
            );
        }
    }

    println!();

    Ok(())
}
