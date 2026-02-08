//! Pack data into a Strata archive.
//!
//! This command creates a `.st` archive from disk and/or memory dumps,
//! calling the core packing logic from `strata_core::ops::pack`.

use crate::ui::progress::create_progress_bar;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use strata_core::ops::pack::{PackConfig, pack_snapshot};

/// Execute the pack command.
#[allow(clippy::too_many_arguments)]
pub fn run(
    disk: Option<PathBuf>,
    memory: Option<PathBuf>,
    output: PathBuf,
    compression: String,
    encrypt: bool,
    train_dict: bool,
    block_size: u32,
    cdc: bool,
    min_chunk: u32,
    avg_chunk: u32,
    max_chunk: u32,
) -> Result<()> {
    // Get password if encryption is enabled
    let password = if encrypt {
        Some(rpassword::prompt_password("Enter encryption password: ")?)
    } else {
        None
    };

    // Calculate total size for progress bar
    let total_size = {
        let mut size = 0u64;
        if let Some(ref path) = disk {
            size += std::fs::metadata(path)?.len();
        }
        if let Some(ref path) = memory {
            size += std::fs::metadata(path)?.len();
        }
        size
    };

    // Create progress bar
    let pb = create_progress_bar(total_size);
    let pb = Arc::new(Mutex::new(pb));
    let pb_clone = pb.clone();

    if train_dict {
        println!("Training compression dictionary...");
    }

    // Create pack configuration
    let config = PackConfig {
        disk,
        memory,
        output: output.clone(),
        compression,
        encrypt,
        password,
        train_dict,
        block_size,
        cdc_enabled: cdc,
        min_chunk,
        avg_chunk,
        max_chunk,
    };

    // Run the packing operation with progress callback
    pack_snapshot(
        config,
        Some(move |current, _total| {
            if let Ok(pb) = pb_clone.lock() {
                pb.set_position(current);
            }
        }),
    )?;

    if let Ok(pb) = pb.lock() {
        pb.finish_with_message("Done");
    }

    println!("Archive created: {:?}", output);
    Ok(())
}
