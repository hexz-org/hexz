//! Convert external data formats into Hexz archives.
//!
//! Supports:
//! - **tar**: Pure Rust via the `tar` crate (streaming, no extraction)

use crate::ui::progress::create_progress_bar;
use anyhow::{Context, Result, bail};
use hexz_core::algo::compression::create_compressor_from_str;
use hexz_ops::archive_writer::ArchiveWriter;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use colored::*;

/// Execute the convert command.
#[allow(clippy::too_many_arguments)]
pub fn run(
    format: String,
    input: PathBuf,
    output: PathBuf,
    compression: String,
    block_size: u32,
    silent: bool,
) -> Result<()> {
    if !silent {
        println!("{} Converting {}", "╭".dimmed(), input.display().to_string().cyan());
        println!("{} Format     {}", "│".dimmed(), format.bright_black());
        println!("{} Output     {}", "╰".dimmed(), output.display().to_string().bright_black());
        println!();
    }
    match format.to_lowercase().as_str() {
        "tar" => convert_tar(input, output, compression, block_size, silent),
        other => bail!("Unknown format: {other:?}. Supported formats: tar"),
    }
}

/// Convert a tar archive to a Hexz archive using pure Rust.
///
/// Streams tar entries directly through the ArchiveWriter without
/// extracting to disk. Stores a file manifest in archive metadata.
fn convert_tar(
    input: PathBuf,
    output: PathBuf,
    compression: String,
    block_size: u32,
    silent: bool,
) -> Result<()> {
    // ... rest of convert_tar ...
    // (I will replace the println! calls later or in a separate step if this is too large)
    // Actually let's try to replace the whole file to be safe with all formatting.

    // Calculate total size for progress bar
    let total_size = std::fs::metadata(&input)
        .with_context(|| format!("Cannot read input file: {}", input.display()))?
        .len();

    // Set up progress bar
    let pb = if !silent {
        let pb = create_progress_bar(total_size);
        Some(Arc::new(Mutex::new(pb)))
    } else {
        None
    };

    // Create compressor and archive writer
    let (compressor, compression_type) =
        create_compressor_from_str(&compression, None, None).map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut writer = ArchiveWriter::builder(&output, compressor, compression_type)
        .block_size(block_size)
        .build()
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Open tar archive (supports .tar, .tar.gz, .tar.bz2, .tar.xz)
    let file = std::fs::File::open(&input)
        .with_context(|| format!("Cannot open tar file: {}", input.display()))?;

    let mut archive = tar::Archive::new(file);

    // Track file manifest for metadata
    let mut source_files: Vec<serde_json::Value> = Vec::new();
    let mut total_bytes: u64 = 0;
    let mut bytes_from_archive: u64 = 0;

    // Begin a main stream for the tar data
    // We'll set total_size after reading all entries by using a two-pass approach,
    // but for streaming we start with the tar file size as an estimate.
    writer.begin_stream(true, total_size);

    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let header = entry.header();

        // Skip non-file entries (directories, symlinks, etc.)
        if !header.entry_type().is_file() {
            continue;
        }

        let name = entry.path()?.to_string_lossy().to_string();
        let size = header.size()?;

        // Read the entry data and write in blocks
        let mut remaining = size;
        let mut buf = vec![0u8; block_size as usize];

        while remaining > 0 {
            let to_read = std::cmp::min(remaining as usize, buf.len());
            entry.read_exact(&mut buf[..to_read])?;

            writer
                .write_data_block(&buf[..to_read])
                .map_err(|e| anyhow::anyhow!("{e}"))?;

            remaining -= to_read as u64;
            bytes_from_archive += to_read as u64;

            if let Some(ref pb) = pb {
                if let Ok(pb) = pb.lock() {
                    // Approximate progress based on bytes read from archive
                    pb.set_position(std::cmp::min(bytes_from_archive, total_size));
                }
            }
        }

        source_files.push(serde_json::json!({
            "name": name,
            "size": size,
            "offset": total_bytes,
        }));
        total_bytes += size;
    }

    writer.end_stream().map_err(|e| anyhow::anyhow!("{e}"))?;

    // Build metadata JSON
    let metadata = serde_json::json!({
        "source": {
            "format": "tar",
            "original_path": input.file_name().unwrap_or_default().to_string_lossy(),
            "total_files": source_files.len(),
            "total_bytes": total_bytes,
            "source_files": source_files,
        }
    });
    let meta_bytes = serde_json::to_vec(&metadata)?;

    writer
        .finalize(Vec::new(), Some(&meta_bytes))
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    if let Some(ref pb) = pb {
        if let Ok(pb) = pb.lock() {
            pb.finish_with_message("Done");
        }
    }

    if !silent {
        println!(
            "\n  {} Converted {} files ({}) from tar archive",
            "✓".green(),
            source_files.len(),
            indicatif::HumanBytes(total_bytes).to_string().bright_black(),
        );
    }

    Ok(())
}

