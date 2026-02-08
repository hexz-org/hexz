//! Show differences in overlay.
//!
//! Analyzes an overlay file to display modified blocks and statistics.

use anyhow::Result;
use indicatif::HumanBytes;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

/// Overlay block granularity (4 KiB).
const OVERLAY_BLOCK_SIZE: u64 = 4096;
/// Size of a metadata entry (8 bytes).
const META_ENTRY_SIZE: usize = 8;

/// Execute the diff command.
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
