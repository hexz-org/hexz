//! Pack data into a Hexz archive.
//!
//! This command creates a `.st` archive from disk images and/or memory dumps,
//! delegating to the core packing logic in [`hexz_ops::pack`].
//!
//! # Overview
//!
//! The `pack` command is the primary way to create Hexz archives from raw data.
//! It supports multiple compression algorithms, encryption, deduplication, and
//! both fixed-size and content-defined chunking strategies.
//!
//! # Key Features
//!
//! - **Compression**: LZ4 (speed) or Zstandard (compression ratio)
//! - **Deduplication**: SHA-256 based block deduplication (enabled by default)
//! - **Encryption**: AES-256-GCM with password-based key derivation
//! - **CDC**: Content-defined chunking for better deduplication
//! - **Dictionary Training**: Improves Zstd compression by 10-30%
//!
//! # Performance Tuning
//!
//! **For maximum speed:**
//! ```bash
//! hexz data pack --disk image.img --output fast.st --compression lz4
//! ```
//!
//! **For maximum compression:**
//! ```bash
//! hexz data pack --disk image.img --output small.st \
//!   --compression zstd --train-dict --cdc
//! ```
//!
//! **For balanced performance:**
//! ```bash
//! hexz data pack --disk image.img --output balanced.st \
//!   --compression lz4 --block-size 131072
//! ```

use crate::ui::progress::create_progress_bar;
use anyhow::Result;
use hexz_ops::pack::{PackConfig, pack_snapshot};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Execute the pack command to create a Hexz snapshot archive.
///
/// This command creates a `.st` snapshot file from disk and/or memory dump files.
/// It supports compression (LZ4 or Zstd), optional encryption, deduplication,
/// content-defined chunking (CDC), and dictionary training for improved compression.
///
/// # Workflow
///
/// The packing process follows these steps:
///
/// 1. **Password prompt** (if encryption enabled): Prompts for password and derives encryption key
/// 2. **Dictionary training** (if enabled): Samples blocks and trains a Zstd compression dictionary
/// 3. **Chunking**: Splits input file(s) into blocks using fixed-size or CDC chunking
/// 4. **Compression**: Compresses each block using the selected algorithm and optional dictionary
/// 5. **Deduplication**: Hashes compressed blocks and eliminates duplicates (enabled by default)
/// 6. **Index building**: Constructs the master index with page entries and block metadata
/// 7. **Header writing**: Serializes header with format version, offsets, and feature flags
///
/// # Arguments
///
/// * `disk` - Optional path to disk image file (raw or qcow2)
/// * `memory` - Optional path to memory dump file
/// * `output` - Output path for the `.st` snapshot file
/// * `compression` - Compression algorithm: "lz4" (fast) or "zstd" (balanced)
/// * `encrypt` - Enable AES-256-GCM encryption (prompts for password)
/// * `train_dict` - Train a Zstd dictionary for improved compression ratios
/// * `block_size` - Block size in bytes (default: 64 KiB)
/// * `cdc` - Enable content-defined chunking for variable-sized blocks
/// * `min_chunk` - Minimum chunk size for CDC (default: 16 KiB)
/// * `avg_chunk` - Average chunk size for CDC (default: 64 KiB)
/// * `max_chunk` - Maximum chunk size for CDC (default: 128 KiB)
/// * `silent` - Suppress progress output
///
/// # Performance Characteristics
///
/// - **LZ4**: ~500 MB/s compression throughput
/// - **Zstd level 3**: ~200 MB/s compression throughput
/// - **Deduplication overhead**: ~5-10% additional time for hashing
/// - **Dictionary training**: 2-5 seconds for typical datasets
///
/// # Example
///
/// ```no_run
/// # use std::path::PathBuf;
/// # use hexz_cli::cmd::data::pack;
/// // Pack a disk image with Zstd compression and dictionary training
/// pack::run(
///     Some(PathBuf::from("disk.img")),
///     None,
///     PathBuf::from("snapshot.hxz"),
///     "zstd".to_string(),
///     false,  // no encryption
///     true,   // train dictionary
///     65536,  // 64 KiB blocks
///     None,   // min chunk (auto-detected)
///     None,   // avg chunk (auto-detected)
///     None,   // max chunk (auto-detected)
///     None,   // workers (auto)
///     false,  // show progress
/// );
/// ```
#[allow(clippy::too_many_arguments)]
pub fn run(
    disk: Option<PathBuf>,
    memory: Option<PathBuf>,
    output: PathBuf,
    compression: String,
    encrypt: bool,
    train_dict: bool,
    block_size: u32,
    min_chunk: Option<u32>,
    avg_chunk: Option<u32>,
    max_chunk: Option<u32>,
    workers: Option<usize>,
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
        let pb = create_progress_bar(total_size);
        let pb = Arc::new(Mutex::new(pb));
        Some(pb)
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
        ..Default::default()
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
        println!("Archive created: {:?}", output);
    }
    Ok(())
}
