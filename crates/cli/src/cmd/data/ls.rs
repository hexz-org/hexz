//! List Hexz archives in a directory and render their lineage as a tree.

use anyhow::{Context, Result};
use hexz_core::format::header::Header;
use hexz_core::format::index::MasterIndex;
use indicatif::HumanBytes;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::{Path, PathBuf};

use crate::ui::color::{Palette, palette};
use colored::Colorize;

struct ArchiveInfo {
    path: PathBuf,
    file_size: u64,
    /// First declared parent path (if any).
    parent: Option<String>,
    /// Number of data blocks (not parent-refs, not sparse).
    data_blocks: usize,
}

fn read_archive_info(path: &Path) -> Result<ArchiveInfo> {
    use std::io::{Read, Seek, SeekFrom};

    use hexz_core::format::index::IndexPage;

    let mut f = File::open(path)?;
    let file_size = f.metadata()?.len();
    let header = Header::read_from(&mut f)?;
    let master = MasterIndex::read_from(&mut f, header.index_offset)?;

    let parent = header.parent_paths.into_iter().next();

    let mut data_blocks = 0usize;
    for page_meta in &master.main_pages {
        let _ = f.seek(SeekFrom::Start(page_meta.offset))?;
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

    let (ann_text, ann_color) = if let Some(ext) = external_parent[idx] {
        let parent_name = Path::new(ext)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        (format!("← {parent_name} (external)"), p.gray)
    } else if a.parent.is_none() {
        ("standalone".to_string(), p.gray)
    } else {
        (format!("+{} new blocks", a.data_blocks), p.dim)
    };

    let name_padded = format!("{name:<32}");
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

    let segment = if is_last {
        "    ".to_string()
    } else {
        format!("{}│{}   ", p.gray, p.reset)
    };
    let child_prefix = format!("{prefix}{segment}");

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

/// Execute the `hexz log` command to list archives and their lineage.
pub fn run(dir: &Path) -> Result<()> {
    let entries: Vec<ArchiveInfo> = std::fs::read_dir(dir)
        .with_context(|| format!("Cannot read directory: {}", dir.display()))?
        .filter_map(std::result::Result::ok)
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

    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut has_parent: HashSet<usize> = HashSet::new();
    for (i, p) in parent_idx.iter().enumerate() {
        if let Some(pi) = p {
            children.entry(*pi).or_default().push(i);
            let _ = has_parent.insert(i);
        }
    }

    let mut roots: Vec<usize> = (0..entries.len())
        .filter(|i| !has_parent.contains(i))
        .collect();
    roots.sort_by_key(|&i| &entries[i].path);

    let p = palette();
    let total_size: u64 = entries.iter().map(|a| a.file_size).sum();

    let dir_str = dir.to_string_lossy();
    let dir_base = dir_str.trim_end_matches('/');
    println!("{} {}/", "╭".dimmed(), dir_base.cyan());

    for (i, &root) in roots.iter().enumerate() {
        let last = i == roots.len() - 1;
        print_tree(
            root,
            &entries,
            &children,
            &external_parent,
            "│ ".dimmed().to_string().as_str(),
            last,
            p,
        );
    }

    let archive_count = format!(
        "{} archive{}",
        entries.len(),
        if entries.len() == 1 { "" } else { "s" }
    );
    println!(
        "{} {}   {} on disk",
        "╰".dimmed(),
        archive_count.bright_black(),
        HumanBytes(total_size).to_string().green(),
    );

    Ok(())
}
