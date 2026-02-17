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
use hexz_core::ops::inspect::inspect_snapshot;
use indicatif::HumanBytes;
use std::path::PathBuf;

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
/// use hexz_cli::cmd::data::info;
///
/// // Display human-readable snapshot information
/// info::run(PathBuf::from("snapshot.hxz"), false)?;
///
/// // Output JSON for automated processing
/// info::run(PathBuf::from("snapshot.hxz"), true)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run(snap: PathBuf, json: bool) -> Result<()> {
    let info = inspect_snapshot(&snap).context("Failed to inspect snapshot")?;

    let total_uncompressed = info.total_uncompressed();
    let ratio = info.compression_ratio();

    if json {
        println!("{{");
        println!("  \"path\": {:?},", snap);
        println!("  \"version\": {},", info.version);
        println!("  \"compression\": {:?},", info.compression);
        println!("  \"block_size\": {},", info.block_size);
        println!("  \"encrypted\": {},", info.encrypted);
        println!("  \"has_disk\": {},", info.has_disk);
        println!("  \"has_memory\": {},", info.has_memory);
        println!("  \"variable_blocks\": {},", info.variable_blocks);
        println!("  \"original_size\": {},", total_uncompressed);
        println!("  \"compressed_size\": {},", info.file_size);
        println!("  \"compression_ratio\": {:.2},", ratio);
        println!("  \"index_offset\": {},", info.index_offset);
        println!("  \"primary_pages\": {},", info.primary_pages);
        println!("  \"secondary_pages\": {}", info.secondary_pages);
        println!("}}");
    } else {
        println!("Snapshot:       {:?}", snap);
        println!("Format Version: {}", info.version);
        println!("Compression:    {:?}", info.compression);
        println!("Block Size:     {}", info.block_size);

        println!("\n--- Features ---");
        println!(
            "Encrypted:      {}",
            if info.encrypted { "Yes" } else { "No" }
        );
        println!(
            "Has Disk:       {}",
            if info.has_disk { "Yes" } else { "No" }
        );
        println!(
            "Has Memory:     {}",
            if info.has_memory { "Yes" } else { "No" }
        );
        println!(
            "Variable Blks:  {}",
            if info.variable_blocks {
                "Yes (CDC)"
            } else {
                "No"
            }
        );

        println!("\n--- Storage Statistics ---");
        println!("Original Size:  {}", HumanBytes(total_uncompressed));
        println!("Compressed:     {}", HumanBytes(info.file_size));
        println!("Ratio:          {:.2}x", ratio);

        println!("\n--- Index Details ---");
        println!("Index Offset:   {}", info.index_offset);
        println!("Disk Pages:     {}", info.primary_pages);
        println!("Memory Pages:   {}", info.secondary_pages);
    }

    Ok(())
}
