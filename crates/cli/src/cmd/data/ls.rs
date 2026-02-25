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

struct ArchiveInfo {
    path: PathBuf,
    file_size: u64,
    /// First declared parent path (if any).
    parent: Option<String>,
    /// Number of data blocks (not parent-refs, not sparse).
    data_blocks: usize,
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

    Ok(ArchiveInfo {
        path: path.to_path_buf(),
        file_size,
        parent,
        data_blocks,
    })
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
                // Try matching on the bare filename of the declared parent path.
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
    let total_size: u64 = entries.iter().map(|a| a.file_size).sum();

    println!();
    println!("  {}/", dir.display());

    fn print_tree(
        idx: usize,
        entries: &[ArchiveInfo],
        children: &HashMap<usize, Vec<usize>>,
        external_parent: &[Option<&str>],
        prefix: &str,
        is_last: bool,
    ) {
        let a = &entries[idx];
        let connector = if is_last { "└──" } else { "├──" };
        let name = a.path.file_name().unwrap_or_default().to_string_lossy();

        let annotation = if let Some(ext) = external_parent[idx] {
            // External parent — show where it comes from
            let parent_name = Path::new(ext)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            format!("← {} (external)", parent_name)
        } else if a.parent.is_none() {
            "standalone".to_string()
        } else {
            format!("+{} new blocks", a.data_blocks)
        };

        println!(
            "  {}{} {:<32} {:>10}  {}",
            prefix,
            connector,
            name,
            HumanBytes(a.file_size).to_string(),
            annotation,
        );

        let mut kids = children.get(&idx).cloned().unwrap_or_default();
        kids.sort_by_key(|&i| &entries[i].path);

        let child_prefix = format!("{}{}   ", prefix, if is_last { " " } else { "│" });
        for (j, &child) in kids.iter().enumerate() {
            let last = j == kids.len() - 1;
            print_tree(
                child,
                entries,
                children,
                external_parent,
                &child_prefix,
                last,
            );
        }
    }

    for (i, &root) in roots.iter().enumerate() {
        let last = i == roots.len() - 1;
        print_tree(root, &entries, &children, &external_parent, "", last);
    }

    println!();
    println!(
        "  {} archive{}   {} on disk",
        entries.len(),
        if entries.len() == 1 { "" } else { "s" },
        HumanBytes(total_size),
    );
    println!();

    Ok(())
}
