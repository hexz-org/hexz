//! Pack data into a Hexz archive.

use crate::ui::progress::create_progress_bar;
use anyhow::Result;
use hexz_ops::pack::{PackConfig, pack_archive};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use colored::*;

/// Execute the pack command to create a Hexz archive archive.
#[allow(clippy::too_many_arguments)]
pub fn run(
    input: Option<PathBuf>,
    base: Option<PathBuf>,
    output: PathBuf,
    compression: String,
    encrypt: bool,
    train_dict: bool,
    block_size: u32,
    min_chunk: Option<u32>,
    avg_chunk: Option<u32>,
    max_chunk: Option<u32>,
    workers: Option<usize>,
    dcam: bool,
    dcam_optimal: bool,
    silent: bool,
) -> Result<()> {
    // Get password if encryption is enabled
    let password = if encrypt {
        Some(match std::env::var("HEXZ_PASSWORD") {
            Ok(p) => p,
            Err(_) => rpassword::prompt_password("Enter encryption password: ")?,
        })
    } else {
        None
    };

    let input_path = input.unwrap_or_else(|| PathBuf::from("."));

    // Calculate total size for progress bar
    let total_size = if input_path.is_dir() {
        walkdir::WalkDir::new(&input_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e: &walkdir::DirEntry| e.file_type().is_file())
            .map(|e: walkdir::DirEntry| e.metadata().map(|m| m.len()).unwrap_or(0))
            .sum()
    } else {
        std::fs::metadata(&input_path).map(|m| m.len()).unwrap_or(0)
    };

    // Setup UI
    if !silent {
        println!("{} Packing {}", "╭".dimmed(), output.display().to_string().cyan());
        println!("{} Input   {}", "│".dimmed(), input_path.display().to_string().bright_black());
        if let Some(ref b) = base {
            println!("{} Base    {}", "│".dimmed(), b.display().to_string().bright_black());
        }
        println!("{}", "╰".dimmed());
    }

    // Create progress bar
    let pb = if !silent {
        let pb = create_progress_bar(total_size);
        Some(Arc::new(Mutex::new(pb)))
    } else {
        None
    };
    let pb_clone = pb.clone();

    if train_dict && !silent {
        println!("  {} Training compression dictionary...", "→".yellow());
    }

    // Create pack configuration
    let config = PackConfig {
        input: input_path,
        base,
        output: output.clone(),
        compression,
        encrypt,
        password,
        train_dict,
        block_size,
        min_chunk,
        avg_chunk,
        max_chunk,
        parallel: workers != Some(1),
        num_workers: workers.unwrap_or(0),
        use_dcam: dcam,
        dcam_optimal,
        ..Default::default()
    };

    // Run the packing operation with progress callback
    pack_archive(
        config,
        Some(move |current, _| {
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
        println!("\n  {} Archive created.", "✓".green());
    }
    Ok(())
}
