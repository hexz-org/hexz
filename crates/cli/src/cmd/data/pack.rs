//! Pack data into a Hexz archive.

use crate::ui::progress::create_progress_bar;
use anyhow::Result;
use colored::Colorize;
use hexz_ops::pack::{PackAnalysisFlags, PackConfig, PackTransformFlags, pack_archive};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Execute the pack command to create a Hexz archive archive.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
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
            .filter_map(std::result::Result::ok)
            .filter(|e: &walkdir::DirEntry| e.file_type().is_file())
            .map(|e: walkdir::DirEntry| e.metadata().map_or(0, |m| m.len()))
            .sum()
    } else {
        std::fs::metadata(&input_path).map_or(0, |m| m.len())
    };

    // Setup UI
    if !silent {
        println!(
            "{} Packing {}",
            "╭".dimmed(),
            output.display().to_string().cyan()
        );
        println!(
            "{} Input   {}",
            "│".dimmed(),
            input_path.display().to_string().bright_black()
        );
        if let Some(ref b) = base {
            println!(
                "{} Base    {}",
                "│".dimmed(),
                b.display().to_string().bright_black()
            );
        }
        println!("{}", "╰".dimmed());
    }

    // Create progress bar
    let pb = if silent {
        None
    } else {
        let pb = create_progress_bar(total_size);
        Some(Arc::new(Mutex::new(pb)))
    };
    let pb_clone = pb.clone();

    if train_dict && !silent {
        println!("  {} Training compression dictionary...", "→".yellow());
    }

    // Create pack configuration
    let config = PackConfig {
        input: input_path,
        base,
        output,
        compression,
        password,
        block_size,
        min_chunk,
        avg_chunk,
        max_chunk,
        num_workers: workers.unwrap_or(0),
        transform: PackTransformFlags {
            encrypt,
            train_dict,
            parallel: workers != Some(1),
        },
        analysis: PackAnalysisFlags {
            show_progress: true,
            use_dcam: dcam,
            dcam_optimal,
        },
    };

    // Run the packing operation with progress callback
    let cb = move |current: u64, _: u64| {
        if let Some(ref pb) = pb_clone {
            if let Ok(pb) = pb.lock() {
                pb.set_position(current);
            }
        }
    };
    pack_archive(&config, Some(&cb))?;

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
