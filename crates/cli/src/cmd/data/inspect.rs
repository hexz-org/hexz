//! Inspect archive metadata and display archive information.

use anyhow::{Context, Result};
use hexz_ops::inspect::inspect_archive;
use indicatif::HumanBytes;
use std::path::PathBuf;

pub fn run(snap: PathBuf, json: bool) -> Result<()> {
    let info = inspect_archive(&snap).context("Failed to inspect archive")?;

    let total_uncompressed = info.total_uncompressed();
    let ratio = info.compression_ratio();

    if json {
        let out = serde_json::json!({
            "path": snap,
            "version": info.version,
            "compression": info.compression,
            "block_size": info.block_size,
            "encrypted": info.encrypted,
            "has_main": info.has_main,
            "has_auxiliary": info.has_auxiliary,
            "variable_blocks": info.variable_blocks,
            "original_size": total_uncompressed,
            "compressed_size": info.file_size,
            "compression_ratio": ratio,
            "index_offset": info.index_offset,
            "main_pages": info.main_pages,
            "auxiliary_pages": info.auxiliary_pages,
            "parent_paths": info.parent_paths,
            "metadata_len": info.metadata_length,
            "block_stats": info.block_stats,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    let filename = snap
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| snap.display().to_string());

    let comp_name = match info.compression {
        hexz_core::format::header::CompressionType::Lz4 => "LZ4",
        hexz_core::format::header::CompressionType::Zstd => "Zstd",
    };

    use colored::*;
    println!("{} {}", "╭".dimmed(), filename.cyan());
    
    let block_kib = info.block_size / 1024;
    println!(
        "{} format      v{}, {}, {} KiB blocks",
        "│".dimmed(),
        info.version,
        comp_name,
        block_kib,
    );

    println!(
        "{} size        {} on disk, {} uncompressed ({:.2}x)",
        "│".dimmed(),
        HumanBytes(info.file_size).to_string().green(),
        HumanBytes(total_uncompressed).to_string().green(),
        ratio,
    );

    if !info.parent_paths.is_empty() {
        let parent_display = std::path::Path::new(&info.parent_paths[0])
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| info.parent_paths[0].clone());
        println!(
            "{} parent      {}",
            "│".dimmed(),
            parent_display.yellow(),
        );
    }

    if let Some(stats) = &info.block_stats {
        let mut parts = Vec::new();
        if stats.data_blocks > 0 {
            parts.push(format!(
                "{} data ({} unique)",
                stats.data_blocks, stats.unique_blocks
            ));
        }
        if stats.parent_ref_blocks > 0 {
            parts.push(format!("{} parent refs", stats.parent_ref_blocks));
        }
        if stats.zero_blocks > 0 {
            parts.push(format!("{} zero", stats.zero_blocks));
        }
        if !parts.is_empty() {
            println!("{} blocks      {}", "│".dimmed(), parts.join(", "));
        }
    }

    if let Some(len) = info.metadata_length {
        println!("{} metadata    {} bytes", "╰".dimmed(), len);
    } else {
        println!("{}", "╰".dimmed());
    }

    Ok(())
}
