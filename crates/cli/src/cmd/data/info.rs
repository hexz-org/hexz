//! Inspect archive metadata and display snapshot information.
//!
//! This command provides a detailed inspection of Strata snapshot files (`.st`),
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
//! - Disk pages (number of index pages for disk stream)
//! - Memory pages (number of index pages for memory stream)
//!
//! # Common Usage Patterns
//!
//! ```bash
//! # Inspect a snapshot with human-readable output
//! strata info vm-snapshot.st
//!
//! # Get machine-readable JSON for scripting
//! strata info vm-snapshot.st --json | jq .compression_ratio
//!
//! # Verify snapshot integrity
//! strata info corrupted.st  # Will fail if header is malformed
//! ```

use anyhow::{Context, Result};
use indicatif::HumanBytes;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use strata_core::format::header::StrataHeader;
use strata_core::format::index::MasterIndex;
use strata_core::format::magic::HEADER_SIZE;

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
/// - `has_disk`: Disk stream present (boolean)
/// - `has_memory`: Memory stream present (boolean)
/// - `variable_blocks`: CDC chunking enabled (boolean)
/// - `original_size`: Uncompressed size in bytes (integer)
/// - `compressed_size`: File size in bytes (integer)
/// - `compression_ratio`: Compression multiplier (float)
/// - `index_offset`: Master index byte offset (integer)
/// - `disk_pages`: Number of disk index pages (integer)
/// - `memory_pages`: Number of memory index pages (integer)
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
/// use strata_cli::cmd::data::info;
///
/// // Display human-readable snapshot information
/// info::run(PathBuf::from("snapshot.st"), false)?;
///
/// // Output JSON for automated processing
/// info::run(PathBuf::from("snapshot.st"), true)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run(snap: PathBuf, json: bool) -> Result<()> {
    let mut f = File::open(&snap).context("Failed to open snapshot file")?;
    let file_len = f.metadata()?.len();

    let mut header_bytes = [0u8; HEADER_SIZE];
    f.read_exact(&mut header_bytes)
        .context("Failed to read header")?;
    let header: StrataHeader =
        bincode::deserialize(&header_bytes).context("Invalid header format")?;

    f.seek(SeekFrom::Start(header.index_offset))
        .context("Failed to seek to index")?;
    let mut index_bytes = Vec::new();
    f.read_to_end(&mut index_bytes)
        .context("Failed to read master index")?;

    let master: MasterIndex =
        bincode::deserialize(&index_bytes).context("Invalid master index format")?;

    let total_uncompressed = master.disk_size + master.memory_size;
    let ratio = if file_len > 0 {
        total_uncompressed as f64 / file_len as f64
    } else {
        0.0
    };

    if json {
        // JSON output
        println!("{{");
        println!("  \"path\": {:?},", snap);
        println!("  \"version\": {},", header.version);
        println!("  \"compression\": {:?},", header.compression);
        println!("  \"block_size\": {},", header.block_size);
        println!("  \"encrypted\": {},", header.encryption.is_some());
        println!("  \"has_disk\": {},", header.features.has_disk);
        println!("  \"has_memory\": {},", header.features.has_memory);
        println!(
            "  \"variable_blocks\": {},",
            header.features.variable_blocks
        );
        println!("  \"original_size\": {},", total_uncompressed);
        println!("  \"compressed_size\": {},", file_len);
        println!("  \"compression_ratio\": {:.2},", ratio);
        println!("  \"index_offset\": {},", header.index_offset);
        println!("  \"disk_pages\": {},", master.disk_pages.len());
        println!("  \"memory_pages\": {}", master.memory_pages.len());
        println!("}}");
    } else {
        // Human-readable output
        println!("Snapshot:       {:?}", snap);
        println!("Format Version: {}", header.version);
        println!("Compression:    {:?}", header.compression);
        println!("Block Size:     {}", header.block_size);

        println!("\n--- Features ---");
        println!(
            "Encrypted:      {}",
            if header.encryption.is_some() {
                "Yes"
            } else {
                "No"
            }
        );
        println!(
            "Has Disk:       {}",
            if header.features.has_disk {
                "Yes"
            } else {
                "No"
            }
        );
        println!(
            "Has Memory:     {}",
            if header.features.has_memory {
                "Yes"
            } else {
                "No"
            }
        );
        println!(
            "Variable Blks:  {}",
            if header.features.variable_blocks {
                "Yes (CDC)"
            } else {
                "No"
            }
        );

        println!("\n--- Storage Statistics ---");
        println!("Original Size:  {}", HumanBytes(total_uncompressed));
        println!("Compressed:     {}", HumanBytes(file_len));
        println!("Ratio:          {:.2}x", ratio);

        println!("\n--- Index Details ---");
        println!("Index Offset:   {}", header.index_offset);
        println!("Disk Pages:     {}", master.disk_pages.len());
        println!("Memory Pages:   {}", master.memory_pages.len());
    }

    Ok(())
}
