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

/// Print metadata lines for a checkpoint: checkpoint info, scalars, message.
///
/// For non-checkpoint metadata, falls back to showing the byte length.
fn print_metadata_lines(raw: &str) {
    if let Ok(obj) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(ver) = obj.get("hexz_checkpoint").and_then(|v| v.as_str()) {
            let tensors = obj
                .get("tensors")
                .and_then(|v| v.as_object())
                .map(|m| m.len())
                .unwrap_or(0);
            println!("  checkpoint: v{}, {} tensors", ver, tensors);

            if let Some(scalars) = obj.get("scalars").and_then(|v| v.as_object()) {
                if !scalars.is_empty() {
                    println!("  scalars:    {}", format_scalars_summary(scalars));
                }
            }

            if let Some(msg) = obj.get("message").and_then(|v| v.as_str()) {
                println!("  message:    {}", msg);
            }

            return;
        }
    }
    println!("  metadata:   {} bytes", raw.len());
}

/// Executes the info command to display snapshot metadata.
///
/// Reads and parses the snapshot header and master index, then displays
/// comprehensive metadata about the snapshot's format, compression, features,
/// and storage statistics. Output can be formatted as human-readable text or
/// JSON for machine consumption.
///
/// # Arguments
///
/// * `snap` - Path to the `.st` snapshot file to inspect
/// * `json` - If true, output JSON format; otherwise, human-readable format
///
/// # Output Details
///
/// **Human-Readable Format:**
/// Displays formatted output with sections for Features and Storage Statistics,
/// using human-friendly byte sizes (e.g., "10.5 GB") and clearly labeled fields.
///
/// **JSON Format:**
/// Outputs a single JSON object with fields:
/// - `path`: Snapshot file path (string)
/// - `version`: Format version number (integer)
/// - `compression`: Compression algorithm ("Lz4" or "Zstd")
/// - `block_size`: Block size in bytes (integer)
/// - `encrypted`: Encryption status (boolean)
/// - `has_disk`: Primary stream present (boolean)
/// - `has_memory`: Secondary stream present (boolean)
/// - `variable_blocks`: CDC chunking enabled (boolean)
/// - `original_size`: Uncompressed size in bytes (integer)
/// - `compressed_size`: File size in bytes (integer)
/// - `compression_ratio`: Compression multiplier (float)
/// - `index_offset`: Master index byte offset (integer)
/// - `primary_pages`: Number of disk index pages (integer)
/// - `secondary_pages`: Number of memory index pages (integer)
///
/// # Errors
///
/// Returns an error if:
/// - The snapshot file cannot be opened (file not found, permission denied)
/// - The header cannot be read (file too small, I/O error)
/// - The header format is invalid (corrupted file, wrong format)
/// - The master index cannot be read (corrupted index, truncated file)
/// - The master index format is invalid (version mismatch, corrupted data)
///
/// # Examples
///
/// ```no_run
/// use std::path::PathBuf;
/// use hexz_cli::cmd::data::inspect;
///
/// // Display human-readable snapshot information
/// inspect::run(PathBuf::from("snapshot.hxz"), false)?;
///
/// // Output JSON for automated processing
/// inspect::run(PathBuf::from("snapshot.hxz"), true)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run(snap: PathBuf, json: bool) -> Result<()> {
    // Note: inspect_snapshot in hexz_core needs to parse the full index
    // to return the block_stats every time.
    let info = inspect_snapshot(&snap).context("Failed to inspect snapshot")?;

    let total_uncompressed = info.total_uncompressed();
    let ratio = info.compression_ratio();

    if json {
        // Output machine-readable JSON
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
    } else {
        // Compact human-readable output
        let filename = snap
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| snap.display().to_string());

        let comp_name = match info.compression {
            hexz_core::format::header::CompressionType::Lz4 => "LZ4",
            hexz_core::format::header::CompressionType::Zstd => "Zstd",
        };

        println!("{}", filename);
        let block_kib = info.block_size / 1024;
        println!(
            "  format:     v{}, {}, {} KiB blocks",
            info.version, comp_name, block_kib,
        );
        println!(
            "  size:       {} on disk, {} uncompressed ({:.2}x)",
            HumanBytes(info.file_size),
            HumanBytes(total_uncompressed),
            ratio,
        );

        if !info.parent_paths.is_empty() {
            // Show just the filename of the first parent
            let parent_display = std::path::Path::new(&info.parent_paths[0])
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| info.parent_paths[0].clone());
            println!("  parent:     {}", parent_display);
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
                println!("  blocks:     {}", parts.join(", "));
            }
        }

        // Metadata summary
        if let Some(meta) = &info.metadata {
            print_metadata_lines(meta);
        }
    }

    Ok(())
}
