//! Inspect FUSE overlay files and identify modified blocks.
//!
//! This command analyzes overlay files created by the FUSE mount (in read-write
//! mode) to display which blocks have been modified. The overlay tracks writes at
//! 4 KiB granularity via a separate `.meta` sidecar file, allowing fast inspection
//! without scanning the (potentially large) overlay data file.
//!
//! # Common Usage
//!
//! ```bash
//! hexz overlay vm-state.overlay             # summary
//! hexz overlay vm-state.overlay --blocks    # block count + size
//! hexz overlay vm-state.overlay --files     # list individual block indices
//! ```

use anyhow::Result;
use hexz_common::constants::{META_ENTRY_SIZE, OVERLAY_BLOCK_SIZE};
use indicatif::HumanBytes;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

/// Show statistics or block indices for a FUSE overlay file.
pub fn run(overlay: PathBuf, blocks: bool, files: bool) -> Result<()> {
    let meta_path = overlay.with_extension("meta");
    if !meta_path.exists() {
        println!("No metadata file found for overlay: {}", overlay.display());
        return Ok(());
    }

    let mut f = File::open(&meta_path)?;
    let count = f.metadata()?.len() / META_ENTRY_SIZE as u64;

    if blocks {
        println!("Modified Blocks: {}", count);
        println!(
            "Total Changed:   {}",
            HumanBytes(count * OVERLAY_BLOCK_SIZE)
        );
    } else if files {
        println!("Modified Block Indices:");
        let mut buf = [0u8; META_ENTRY_SIZE];
        f.seek(SeekFrom::Start(0))?;
        for _ in 0..count {
            if f.read_exact(&mut buf).is_ok() {
                println!("  {}", u64::from_le_bytes(buf));
            }
        }
    } else {
        println!("Overlay:  {}", overlay.display());
        println!(
            "Modified: {}  ({})",
            count,
            HumanBytes(count * OVERLAY_BLOCK_SIZE)
        );
    }

    Ok(())
}
