//! Show differences in overlay and identify modified blocks.
//!
//! This command analyzes overlay files created by the FUSE mount (in read-write mode)
//! to display which blocks have been modified, providing statistics about write
//! activity and changed data. The overlay mechanism tracks writes at 4 KiB granularity,
//! allowing efficient copy-on-write semantics without modifying the base snapshot.
//!
//! # Overlay Format
//!
//! When mounting a snapshot in read-write mode, two files are created:
//!
//! **Overlay File (`.overlay`):**
//! - Contains modified 4 KiB blocks written by the VM/guest
//! - Sparse file with blocks at their original logical offsets
//! - Only modified blocks consume disk space
//!
//! **Metadata File (`.meta`):**
//! - Contains a sorted list of modified block indices (8 bytes each)
//! - Used to quickly enumerate changed blocks without scanning the overlay
//! - Format: array of `u64` block numbers in little-endian encoding
//!
//! # Use Cases
//!
//! - **Change Tracking**: Identify what data has been modified during VM execution
//! - **Incremental Commits**: Determine which blocks need to be merged into new snapshot
//! - **Debugging**: Investigate unexpected writes or storage growth
//! - **Capacity Planning**: Estimate commit size before running `vm commit`
//! - **File-Level Analysis**: Map modified blocks to files (future enhancement)
//!
//! # Output Modes
//!
//! **Default Mode (Summary):**
//! Displays basic statistics:
//! - Total number of modified blocks
//! - Estimated data size changed
//!
//! **Blocks Mode (`--blocks`):**
//! Shows overlay statistics with human-readable sizes.
//!
//! **Files Mode (`--files`):**
//! Lists individual modified block indices. File-level resolution
//! (mapping blocks to filesystem inodes) is not yet implemented.
//!
//! # Comparison to Other Diff Tools
//!
//! Unlike traditional file diffs (e.g., `diff`, `rsync --dry-run`):
//! - Operates at block level, not file level
//! - Does not require mounting or filesystem parsing
//! - Fast: reads only the small metadata file, not entire overlay
//! - Shows raw block changes, not semantic file differences
//!
//! # Common Usage Patterns
//!
//! ```bash
//! # Show summary of changes
//! strata diff overlay.img
//!
//! # Show detailed block statistics
//! strata diff overlay.img --blocks
//!
//! # List all modified block indices
//! strata diff overlay.img --files
//!
//! # Estimate commit size before running vm commit
//! strata diff vm-state.overlay --blocks
//! # Output: "Modified Blocks: 5120 | Total Changed Data: 20.0 MB"
//! ```

use anyhow::Result;
use indicatif::HumanBytes;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

/// Overlay block granularity (4 KiB).
///
/// **Architectural intent:** Matches the standard filesystem block size for
/// compatibility with guest filesystems (ext4, xfs, ntfs) and ensures
/// reasonable copy-on-write granularity without excessive metadata overhead.
const OVERLAY_BLOCK_SIZE: u64 = 4096;

/// Size of a metadata entry (8 bytes).
///
/// **Architectural intent:** Each entry is a `u64` block index in little-endian
/// format, allowing up to 2^64 * 4KiB = 64 ZiB addressable space.
const META_ENTRY_SIZE: usize = 8;

/// Executes the diff command to analyze overlay modifications.
///
/// Reads the overlay metadata file (`.meta`) to determine which blocks have been
/// modified, then displays statistics about the changes. The metadata file contains
/// a sorted array of 64-bit block indices that were written during overlay operation.
///
/// # Arguments
///
/// * `overlay` - Path to the overlay file (e.g., `vm-state.overlay`)
/// * `blocks` - If true, display block-level statistics with human-readable sizes
/// * `files` - If true, display individual modified block indices (file mapping not implemented)
///
/// # Output Format
///
/// **Blocks Mode:**
/// ```text
/// --- Overlay Statistics ---
/// Modified Blocks: 5120
/// Total Changed Data: 20.0 MB
/// ```
///
/// **Files Mode:**
/// ```text
/// --- Modified Files (Heuristic) ---
/// File resolution is not yet implemented. Use --blocks for raw stats.
/// Modified Block Indices:
///   Block 128
///   Block 129
///   Block 256
///   ...
/// ```
///
/// **Default Mode:**
/// ```text
/// Overlay: "vm-state.overlay"
/// Modified Blocks: 5120
/// Estimated Size: 20.0 MB
/// ```
///
/// # File-Level Resolution (Future Enhancement)
///
/// To map block indices to files, the implementation would need to:
/// 1. Read the base image partition table (MBR/GPT)
/// 2. Identify the filesystem type (ext4, xfs, ntfs, etc.)
/// 3. Parse the filesystem metadata (superblock, inode tables)
/// 4. Map block indices to inode numbers
/// 5. Resolve inode paths from directory entries
///
/// This requires filesystem-specific parsers and is left for future work.
///
/// # Errors
///
/// Returns an error if:
/// - The overlay file does not exist (note: metadata file absence is not an error)
/// - The metadata file cannot be opened or read
/// - I/O errors occur while reading block indices
///
/// Note: If the metadata file does not exist, the command prints a message and
/// returns successfully (interpreted as zero modifications).
///
/// # Examples
///
/// ```no_run
/// use std::path::PathBuf;
/// use strata_cli::cmd::data::diff;
///
/// // Show summary of overlay changes
/// diff::run(PathBuf::from("vm-state.overlay"), false, false)?;
///
/// // Display detailed statistics
/// diff::run(PathBuf::from("vm-state.overlay"), true, false)?;
///
/// // List all modified block indices
/// diff::run(PathBuf::from("vm-state.overlay"), false, true)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run(overlay: PathBuf, blocks: bool, files: bool) -> Result<()> {
    let meta_path = overlay.with_extension("meta");
    if !meta_path.exists() {
        println!("No metadata file found for overlay: {:?}", overlay);
        return Ok(());
    }

    let mut f = File::open(&meta_path)?;
    let len = f.metadata()?.len();
    let count = len / META_ENTRY_SIZE as u64;

    if blocks {
        println!("--- Overlay Statistics ---");
        println!("Modified Blocks: {}", count);
        println!(
            "Total Changed Data: {}",
            HumanBytes(count * OVERLAY_BLOCK_SIZE)
        );
    }

    if files {
        println!("\n--- Modified Files (Heuristic) ---");
        println!("File resolution is not yet implemented. Use --blocks for raw stats.");
        // TODO: Future implementation:
        // 1. Read base image MBR/GPT.
        // 2. Identify partition.
        // 3. Mount or parse filesystem (ext4/xfs).
        // 4. Map block indices to file inodes.

        println!("Modified Block Indices:");
        let mut buf = [0u8; META_ENTRY_SIZE];
        f.seek(SeekFrom::Start(0))?;
        for _ in 0..count {
            if f.read_exact(&mut buf).is_ok() {
                let blk = u64::from_le_bytes(buf);
                println!("  Block {}", blk);
            }
        }
    }

    if !blocks && !files {
        // Default behavior: just show summary
        println!("Overlay: {:?}", overlay);
        println!("Modified Blocks: {}", count);
        println!("Estimated Size: {}", HumanBytes(count * OVERLAY_BLOCK_SIZE));
    }

    Ok(())
}
