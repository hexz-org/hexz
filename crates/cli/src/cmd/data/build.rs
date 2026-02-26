//! Build archive from source with profile-based optimization.
//!
//! This command provides a high-level interface for creating Hexz snapshots
//! with domain-specific optimizations. Unlike the low-level `pack` command,
//! `build` uses predefined profiles that automatically select compression
//! algorithms, block sizes, and dictionary training settings optimized for
//! different workload types.
//!
//! # Build Profiles
//!
//! Profiles configure compression and chunking parameters based on workload characteristics:
//!
//! **Generic Profile (Default):**
//! - Compression: LZ4 (fast, general-purpose)
//! - Block size: 64 KiB (balanced for most workloads)
//! - Dictionary training: Disabled (minimal overhead)
//! - Use case: General operating system images, file servers, development VMs
//!
//! **EDA Profile (Electronic Design Automation):**
//! - Compression: Zstd level 3 (high ratio for large design files)
//! - Block size: 128 KiB (optimized for large CAD files and netlists)
//! - Dictionary training: Enabled (learns patterns from design data)
//! - Use case: ASIC/FPGA design environments with large binary databases
//!
//! **Embedded Profile:**
//! - Compression: LZ4 (minimal CPU overhead for resource-constrained targets)
//! - Block size: 16 KiB (smaller blocks reduce memory pressure)
//! - Dictionary training: Disabled (reduces snapshot creation time)
//! - Use case: IoT devices, embedded Linux systems, edge computing
//!
//! **ML Profile (Machine Learning):**
//! - Compression: Zstd level 3 (handles large model files efficiently)
//! - Block size: 256 KiB (optimized for model weights and training data)
//! - Dictionary training: Enabled (learns patterns from tensor data)
//! - Use case: ML training environments, GPU workstations, Jupyter notebooks
//!
//! # Build Profile Effects
//!
//! ## Compression Algorithm Selection
//!
//! - **LZ4**: Provides 2-3x compression at 500+ MB/s, ideal for fast boot times
//! - **Zstd**: Provides 3-5x compression at 200+ MB/s, ideal for storage efficiency
//!
//! ## Block Size Selection
//!
//! Smaller blocks (16-64 KiB):
//! - Lower memory usage during decompression
//! - Finer-grained deduplication
//! - Higher index overhead
//!
//! Larger blocks (128-256 KiB):
//! - Better compression ratios (larger context window)
//! - Reduced index size
//! - Higher decompression memory requirements
//!
//! ## Dictionary Training
//!
//! When enabled, samples ~1000 blocks and trains a Zstd dictionary:
//! - Improves compression ratio by 10-30% for repetitive data
//! - Adds 2-5 seconds to snapshot creation time
//! - Dictionary size: ~110 KiB stored in snapshot header
//!
//! # Use Cases
//!
//! - **VM Image Creation**: Build bootable snapshots from disk images
//! - **Reproducible Environments**: Create snapshots with consistent compression settings
//! - **Workload Optimization**: Select profiles matched to application characteristics
//! - **CI/CD Pipelines**: Automate snapshot creation with profile presets
//!
//! # Common Usage Patterns
//!
//! ```bash
//! # Build generic snapshot from disk image
//! hexz build --source disk.img --output snapshot.st
//!
//! # Build EDA workstation with optimal compression
//! hexz build --source eda-vm.img --output eda.st --profile eda
//!
//! # Build ML environment with encryption
//! hexz build --source ml.img --output ml.st --profile ml --encrypt
//!
//! # Build with content-defined chunking for deduplication
//! hexz build --source app.img --output app.st --cdc
//! ```

use anyhow::Result;
use hexz_common::config::BuildProfile;
use std::path::PathBuf;

/// Executes the build command to create a snapshot using profile-based settings.
///
/// This command maps a high-level build profile to low-level packing parameters
/// (compression algorithm, block size, dictionary training) and delegates to the
/// `pack` command for actual snapshot creation. This provides a simplified
/// interface for users who want optimized settings without manual tuning.
///
/// # Arguments
///
/// * `source` - Path to the source disk image (raw or qcow2 format)
/// * `memory` - Optional path to memory dump file to include in snapshot
/// * `output` - Output path for the generated `.st` snapshot file
/// * `profile` - Build profile name: "generic", "eda", "embedded", or "ml"
/// * `encrypt` - Enable AES-256-GCM encryption (prompts for password)
/// * `cdc` - Enable content-defined chunking for variable-sized blocks
///
/// # Profile Parameter Mapping
///
/// The function resolves the profile name to a `BuildProfile` enum and extracts:
/// - Compression algorithm (`compression_algo()`)
/// - Block size in bytes (`block_size()`)
/// - Dictionary training recommendation (`recommended_dict_training()`)
///
/// These parameters are then passed to `pack::run()` along with CDC settings.
///
/// # CDC (Content-Defined Chunking) Parameters
///
/// When `cdc` is enabled, variable-sized blocks are used with FastCDC:
/// - `min_chunk`: 16 KiB minimum chunk size
/// - `avg_chunk`: 64 KiB average chunk size (default block size)
/// - `max_chunk`: 128 KiB maximum chunk size
///
/// These defaults can be overridden by calling `pack::run()` directly.
///
/// # Errors
///
/// Returns an error if:
/// - The source file cannot be opened or read
/// - The output path is not writable
/// - The encryption password is invalid (if encryption is enabled)
/// - Compression or packing operations fail
/// - Disk I/O errors occur during processing
///
/// # Examples
///
/// ```no_run
/// use std::path::PathBuf;
/// use hexz_cli::cmd::data::build;
///
/// // Build generic snapshot without encryption
/// build::run(
///     PathBuf::from("disk.img"),
///     None,
///     PathBuf::from("snapshot.hxz"),
///     Some("generic".to_string()),
///     false,  // no encryption
///     false,  // no CDC
/// )?;
///
/// // Build ML profile with encryption and CDC
/// build::run(
///     PathBuf::from("ml-vm.img"),
///     None,
///     PathBuf::from("ml.hxz"),
///     Some("ml".to_string()),
///     true,   // encrypt
///     true,   // enable CDC
/// )?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run(
    source: PathBuf,
    memory: Option<PathBuf>,
    output: PathBuf,
    profile: Option<String>,
    encrypt: bool,
    cdc: bool,
) -> Result<()> {
    // 1. Resolve profile
    let build_profile = match profile.as_deref() {
        Some("eda") => BuildProfile::Eda,
        Some("embedded") => BuildProfile::Embedded,
        Some("ml") => BuildProfile::Ml,
        Some("generic") | None => BuildProfile::Generic,
        Some(other) => {
            eprintln!(
                "Warning: Unknown profile '{}', falling back to generic",
                other
            );
            BuildProfile::Generic
        }
    };

    println!("Building snapshot with profile: {:?}", build_profile);

    // 2. Map profile to parameters
    let compression = build_profile.compression_algo().to_string();
    let block_size = build_profile.block_size();
    let train_dict = build_profile.recommended_dict_training();

    // 3. Delegate to pack
    // Note: We currently map `source` directly to `disk`.
    // Future work: Detect if `source` is a directory and pack it (e.g. tar/squashfs)
    // or use `virt-make-fs`.
    super::pack::run(
        Some(source),
        memory,
        output,
        compression,
        encrypt,
        train_dict,
        block_size,
        cdc,
        16384,  // min_chunk default
        65536,  // avg_chunk default
        131072, // max_chunk default
        None,   // workers (auto)
        false,  // silent
    )
}
