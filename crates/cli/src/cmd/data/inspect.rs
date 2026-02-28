//! Inspect archive metadata and display snapshot information.
//!
//! This command provides a detailed inspection of Hexz snapshot files (`.st`),
//! reading the file header and master index to display metadata about the
//! snapshot's structure, compression, encryption status, and storage statistics.
//!
//! # Use Cases
//!
//! - **Snapshot Inspection**: Verify snapshot format version and feature flags
//! - **Compression Analysis**: Check compression algorithm and ratio achieved
//! - **Capacity Planning**: View original vs. compressed size for storage estimates
//! - **Debugging**: Identify snapshot corruption or format mismatches
//! - **Automation**: JSON output mode enables scripting and tooling integration
//!
//! # Workflow
//!
//! The command performs these steps:
//!
//! 1. **Header Reading**: Reads the fixed-size header (4096 bytes) from file start
//! 2. **Index Location**: Uses `index_offset` from header to locate the master index
//! 3. **Index Parsing**: Deserializes the master index to extract page metadata
//! 4. **Compression Ratio**: Calculates ratio from uncompressed vs. file size
//! 5. **Output Formatting**: Renders human-readable or JSON output
//!
//! # Output Format
//!
//! The command displays:
//!
//! **Header Information:**
//! - Format version (currently v1)
//! - Compression algorithm (LZ4 or Zstd)
//! - Block size used for chunking
//!
//! **Feature Flags:**
//! - Encryption status (encrypted or plaintext)
//! - Disk presence (whether snapshot contains disk image)
//! - Memory presence (whether snapshot contains memory dump)
//! - Variable blocks (whether CDC chunking was used)
//!
//! **Storage Statistics:**
//! - Original size (sum of uncompressed disk + memory)
//! - Compressed size (total file size on disk)
//! - Compression ratio (multiplier showing space savings)
//!
//! **Index Details:**
//! - Index offset in file (byte position)
//! - Disk pages (number of index pages for primary stream)
//! - Memory pages (number of index pages for secondary stream)
//!
//! # Common Usage Patterns
//!
//! ```bash
//! # Inspect a snapshot with human-readable output
//! hexz info vm-snapshot.st
//!
//! # Get machine-readable JSON for scripting
//! hexz info vm-snapshot.st --json | jq .compression_ratio
//!
//! # Verify snapshot integrity
//! hexz info corrupted.st  # Will fail if header is malformed
//! ```

use anyhow::{Context, Result};
use hexz_ops::inspect::inspect_snapshot;
use indicatif::HumanBytes;
use std::path::PathBuf;

use crate::ui::color::{Palette, palette};

/// Format a scalar value for inline display.
///
/// Floats → 4 decimal places, ints as-is, strings quoted, bools lowercase.
fn format_scalar_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if n.is_f64() {
                    format!("{:.4}", f)
                } else {
                    // Integer stored as Number
                    format!("{}", n)
                }
            } else {
                format!("{}", n)
            }
        }
        serde_json::Value::Bool(b) => format!("{}", b),
        serde_json::Value::String(s) => format!("\"{}\"", s),
        _ => format!("{}", v),
    }
}

/// Format a scalars map into an inline summary like `step=8, loss=2.2702`.
pub fn format_scalars_summary(scalars: &serde_json::Map<String, serde_json::Value>) -> String {
    let mut parts: Vec<String> = scalars
        .iter()
        .map(|(k, v)| {
            let display_val = if let Some(obj) = v.as_object() {
                // Checkpoint format: {"type": "...", "value": ...}
                if let Some(val) = obj.get("value") {
                    format_scalar_value(val)
                } else {
                    format_scalar_value(v)
                }
            } else {
                format_scalar_value(v)
            };
            format!("{}={}", k, display_val)
        })
        .collect();
    parts.sort();
    parts.join(", ")
}

/// Returns a colored, right-padded label for the inspect table.
///
/// The visible column width is always 14 characters (`"  " + key + spaces`),
/// keeping values aligned regardless of ANSI escape sequences.
fn lbl(key: &str, p: &'static Palette) -> String {
    let spaces = 14usize.saturating_sub(2 + key.len());
    format!("  {}{}{}{}", p.cyan, key, p.reset, " ".repeat(spaces))
}

/// Print metadata lines for a checkpoint: checkpoint info, scalars, message.
///
/// For non-checkpoint metadata, falls back to showing the byte length.
fn print_metadata_lines(raw: &str, p: &'static Palette) {
    if let Ok(obj) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(ver) = obj.get("hexz_checkpoint").and_then(|v| v.as_str()) {
            let tensors = obj
                .get("tensors")
                .and_then(|v| v.as_object())
                .map(|m| m.len())
                .unwrap_or(0);
            println!("{}v{}, {} tensors", lbl("checkpoint:", p), ver, tensors);

            if let Some(scalars) = obj.get("scalars").and_then(|v| v.as_object()) {
                if !scalars.is_empty() {
                    println!(
                        "{}{}{}{}",
                        lbl("scalars:", p),
                        p.yellow,
                        format_scalars_summary(scalars),
                        p.reset,
                    );
                }
            }

            if let Some(msg) = obj.get("message").and_then(|v| v.as_str()) {
                println!("{}{}", lbl("message:", p), msg);
            }

            return;
        }
    }
    println!("{}{} bytes", lbl("metadata:", p), raw.len());
}

/// Executes the info command to display snapshot metadata.
pub fn run(snap: PathBuf, json: bool) -> Result<()> {
    let info = inspect_snapshot(&snap).context("Failed to inspect snapshot")?;

    let total_uncompressed = info.total_uncompressed();
    let ratio = info.compression_ratio();

    if json {
        let out = serde_json::json!({
            "path": snap,
            "version": info.version,
            "compression": info.compression,
            "block_size": info.block_size,
            "encrypted": info.encrypted,
            "has_primary": info.has_primary,
            "has_secondary": info.has_secondary,
            "variable_blocks": info.variable_blocks,
            "original_size": total_uncompressed,
            "compressed_size": info.file_size,
            "compression_ratio": ratio,
            "index_offset": info.index_offset,
            "primary_pages": info.primary_pages,
            "secondary_pages": info.secondary_pages,
            "parent_paths": info.parent_paths,
            "metadata": info.metadata,
            "block_stats": info.block_stats,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    let p = palette();

    let filename = snap
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| snap.display().to_string());

    let comp_name = match info.compression {
        hexz_core::format::header::CompressionType::Lz4 => "LZ4",
        hexz_core::format::header::CompressionType::Zstd => "Zstd",
    };

    // Title
    println!("{}{}{}", p.bold, filename, p.reset);

    // Format
    let block_kib = info.block_size / 1024;
    println!(
        "{}v{}, {}, {} KiB blocks",
        lbl("format:", p),
        info.version,
        comp_name,
        block_kib,
    );

    // Size
    println!(
        "{}{}{}{} on disk, {}{}{} uncompressed ({}{:.2}x{})",
        lbl("size:", p),
        p.green,
        HumanBytes(info.file_size),
        p.reset,
        p.green,
        HumanBytes(total_uncompressed),
        p.reset,
        p.bold,
        ratio,
        p.reset,
    );

    // Parent
    if !info.parent_paths.is_empty() {
        let parent_display = std::path::Path::new(&info.parent_paths[0])
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| info.parent_paths[0].clone());
        println!(
            "{}{}{}{}",
            lbl("parent:", p),
            p.yellow,
            parent_display,
            p.reset
        );
    }

    // Block stats
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
            println!("{}{}", lbl("blocks:", p), parts.join(", "));
        }
    }

    // Metadata summary (checkpoint, scalars, message)
    if let Some(meta) = &info.metadata {
        print_metadata_lines(meta, p);
    }

    Ok(())
}
