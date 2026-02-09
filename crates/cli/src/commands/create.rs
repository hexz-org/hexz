//! Snapshot creation from raw disk and optional memory images.

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use strata_core::ops::pack::{PackConfig, pack_snapshot};

#[allow(clippy::too_many_arguments)]
pub fn run(
    disk: Option<PathBuf>,
    memory: Option<PathBuf>,
    output: PathBuf,
    algo: String,
    encrypt: bool,
    train_dict: bool,
    block_size: u32,
    cdc_enabled: bool,
    min_chunk: u32,
    avg_chunk: u32,
    max_chunk: u32,
    silent: bool,
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
    let pb = if !silent {
        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40} {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("=>-"),
        );
        Some(Arc::new(Mutex::new(pb)))
    } else {
        None
    };

    let pb_clone = pb.clone();

    if train_dict && !silent {
        println!("Training compression dictionary...");
    }

    // Create pack configuration
    let config = PackConfig {
        disk,
        memory,
        output: output.clone(),
        compression: algo,
        encrypt,
        password,
        train_dict,
        block_size,
        cdc_enabled,
        min_chunk,
        avg_chunk,
        max_chunk,
    };

    // Run the packing operation with progress callback
    pack_snapshot(
        config,
        Some(move |current, _total| {
            if let Some(ref pb) = pb_clone {
                if let Ok(pb) = pb.lock() {
                    pb.set_position(current);
                }
            }
        }),
    )?;

    if let Some(ref pb) = pb {
        if let Ok(pb) = pb.lock() {
            pb.finish_with_message("Done");
        }
    }

    if !silent {
        println!("Snapshot created at {:?}", output);
    }
    Ok(())
}
