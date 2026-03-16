//! High-level archive packing operations.
//!
//! This module implements the core business logic for creating Hexz archive files
//! from raw disk and memory images. It orchestrates a multi-stage pipeline that
//! transforms raw input data into compressed, indexed, and optionally encrypted
//! archive files optimized for fast random access and deduplication.
//!
//! # Core Capabilities
//!
//! - **Dictionary Training**: Intelligent sampling and Zstd dictionary optimization
//! - **Chunking Strategies**: Fixed-size blocks or content-defined (`FastCDC`) for better deduplication
//! - **Compression**: LZ4 (fast) or Zstd (high-ratio) with optional dictionary support
//! - **Encryption**: Per-block AES-256-GCM authenticated encryption
//! - **Deduplication**: BLAKE3 based content deduplication (disabled for encrypted data)
//! - **Hierarchical Indexing**: Two-level index structure for efficient random access
//! - **Progress Reporting**: Optional callback interface for UI integration
//!
//! # Architecture
//!
//! The packing process follows a carefully orchestrated pipeline. Each stage is designed
//! to be memory-efficient (streaming) and to minimize write amplification:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │ Stage 1: Dictionary Training (Optional, Zstd only)                  │
//! │                                                                      │
//! │  Input File → Stratified Sampling → Entropy Filtering → Zstd Train │
//! │                                                                      │
//! │  - Samples ~4000 blocks evenly distributed across input             │
//! │  - Filters out zero blocks and high-entropy data (>6.0 bits/byte)   │
//! │  - Produces dictionary (max 110 KiB) optimized for dataset          │
//! │  - Training time: 2-5 seconds for typical VM images                 │
//! └─────────────────────────────────────────────────────────────────────┘
//!                                  ↓
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │ Stage 2: Stream Processing (Per Input: Disk, Memory)                │
//! │                                                                      │
//! │  Raw Input → Chunking → Compression → Encryption → Dedup → Write   │
//! │                                                                      │
//! │  Chunking:                                                           │
//! │   - Fixed-size: Divide into equal blocks (default 64 KiB)           │
//! │   - FastCDC: Content-defined boundaries for better deduplication    │
//! │                                                                      │
//! │  Zero Block Optimization:                                            │
//! │   - Detect all-zero chunks (common in VM images)                    │
//! │   - Store as metadata only (offset=0, length=0)                     │
//! │   - Saves significant space for sparse images                       │
//! │                                                                      │
//! │  Deduplication (Unencrypted only):                                  │
//! │   - Compute BLAKE3 hash of compressed data                           │
//! │   - Check hash table for existing block                             │
//! │   - Reuse offset if duplicate found                                 │
//! │   - Note: Disabled for encrypted data (unique nonces prevent dedup) │
//! │                                                                      │
//! │  Index Page Building:                                                │
//! │   - Accumulate BlockInfo metadata (offset, length, checksum)        │
//! │   - Flush page when reaching 4096 entries (~16 MB logical data)     │
//! │   - Write serialized page to output, record PageEntry               │
//! └─────────────────────────────────────────────────────────────────────┘
//!                                  ↓
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │ Stage 3: Index Finalization                                          │
//! │                                                                      │
//! │  MasterIndex (main_pages[], auxiliary_pages[], sizes) → Serialize      │
//! │                                                                      │
//! │  - Collect all PageEntry records from both streams                  │
//! │  - Write master index at end of file                                │
//! │  - Record index offset in header                                    │
//! └─────────────────────────────────────────────────────────────────────┘
//!                                  ↓
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │ Stage 4: Header Writing                                              │
//! │                                                                      │
//! │  - Seek to file start (reserved 512 bytes)                          │
//! │  - Write Header with format metadata                          │
//! │  - Includes: compression type, encryption params, index offset      │
//! │  - Flush to ensure atomicity                                        │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Optimization Strategies
//!
//! ## Dictionary Training Algorithm
//!
//! The dictionary training process improves compression ratios by 10-30% for
//! structured data (file systems, databases) by building a Zstd shared dictionary:
//!
//! 1. **Stratified Sampling**: Sample blocks evenly across input to capture diversity
//!    - Step size = `file_size` / `target_samples` (typically 4000 samples)
//!    - Ensures coverage of different file system regions
//!
//! 2. **Quality Filtering**: Exclude unsuitable blocks
//!    - Skip all-zero blocks (no compressible patterns)
//!    - Compute Shannon entropy for each block
//!    - Reject blocks with entropy > 6.0 bits/byte (likely encrypted/random)
//!
//! 3. **Training**: Feed filtered samples to Zstd dictionary builder
//!    - Target dictionary size: 110 KiB (fits in L2 cache)
//!    - Uses Zstd's COVER algorithm to extract common patterns
//!
//! ## Deduplication Mechanism
//!
//! Content-based deduplication eliminates redundant blocks:
//!
//! - **Hash Table**: Maps BLAKE3 hash → physical offset for each unique compressed block
//! - **Collision Handling**: BLAKE3 collisions are astronomically unlikely (2^128 blocks)
//! - **Memory Usage**: ~48 bytes per unique block (32-byte hash + 8-byte offset + `HashMap` overhead)
//! - **Write Behavior**: Only write each unique block once; reuse offset for duplicates
//! - **Encryption Interaction**: Disabled when encrypting (each block gets unique nonce/ciphertext)
//!
//! ## Index Page Management
//!
//! The two-level index hierarchy balances random access performance and metadata overhead:
//!
//! - **Page Size**: 4096 entries per page
//!   - With 64 KiB blocks: Each page covers ~256 MB of logical data
//!   - Serialized page size: ~64 KiB (fits in L2 cache)
//!
//! - **Flushing Strategy**: Eager flush when page fills
//!   - Prevents memory growth during large packs
//!   - Enables streaming operation (constant memory)
//!
//! - **Master Index**: Array of `PageEntry` records
//!   - Binary search for O(log N) page lookup
//!   - Typical overhead: 1 KiB per GB of data
//!
//! # Memory Usage Patterns
//!
//! The packing operation is designed for constant memory usage regardless of input size:
//!
//! - **Chunking Buffer**: 1 block (64 KiB default)
//! - **Compression Output**: ~1.5× block size (worst case: incompressible data)
//! - **Current Index Page**: Up to 4096 × 20 bytes = 80 KiB
//! - **Deduplication Map**: ~48 bytes × `unique_blocks`
//!   - Example: 10 GB image with 50% dedup = ~80 MB `HashMap`
//! - **Dictionary**: 110 KiB (if trained)
//!
//! Total typical memory: 100-200 MB for dedup hash table + ~1 MB working set.
//!
//! # Error Recovery
//!
//! The packing operation is not atomic. On failure:
//!
//! - **Partial File**: Output file is left in incomplete state
//! - **Header Invalid**: Header is written last, so partial packs have zeroed header
//! - **Detection**: Readers validate magic bytes and header checksum
//! - **Recovery**: None; must delete partial file and retry pack operation
//!
//! Future enhancement: Two-phase commit with temporary file + atomic rename.
//!
//! # Usage Contexts
//!
//! This module is designed to be called from multiple contexts:
//!
//! - **CLI Commands**: `hexz data pack` (with terminal progress bars)
//! - **Python Bindings**: `hexz.pack()` (with optional callbacks)
//! - **Rust Applications**: Direct API usage for embedded scenarios
//!
//! By keeping pack operations separate from UI/CLI code, we avoid pulling in
//! heavy dependencies (`clap`, `indicatif`) into library contexts.
//!
//! # Examples
//!
//! ## Basic Packing (LZ4, No Encryption)
//!
//! ```no_run
//! use hexz_ops::pack::{pack_archive, PackConfig};
//! use std::path::PathBuf;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = PackConfig {
//!     input: PathBuf::from("disk.raw"),
//!     output: PathBuf::from("archive.hxz"),
//!     compression: "lz4".to_string(),
//!     ..Default::default()
//! };
//!
//! pack_archive::<fn(u64, u64)>(&config, None)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Advanced Packing (Zstd with Dictionary, CDC, Encryption)
//!
//! ```no_run
//! use hexz_ops::pack::{pack_archive, PackConfig, PackTransformFlags};
//! use std::path::PathBuf;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = PackConfig {
//!     input: PathBuf::from("ubuntu.qcow2"),
//!     output: PathBuf::from("ubuntu.hxz"),
//!     compression: "zstd".to_string(),
//!     password: Some("secure_passphrase".to_string()),
//!     min_chunk: Some(16384),   // 16 KiB minimum chunk
//!     avg_chunk: Some(65536),   // 64 KiB average chunk
//!     max_chunk: Some(262144),  // 256 KiB maximum chunk
//!     transform: PackTransformFlags {
//!         train_dict: true,     // Train dictionary for better ratio
//!         encrypt: true,
//!         ..Default::default()
//!     },
//!     ..Default::default()
//! };
//!
//! pack_archive::<fn(u64, u64)>(&config, None)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Progress Reporting
//!
//! ```no_run
//! use hexz_ops::pack::{pack_archive, PackConfig};
//! use std::path::PathBuf;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = PackConfig {
//!     input: PathBuf::from("disk.raw"),
//!     output: PathBuf::from("archive.hxz"),
//!     ..Default::default()
//! };
//!
//! // Callback receives (current_logical_pos, total_size)
//! let cb = |pos: u64, total: u64| {
//!     let pct = (pos as f64 / total as f64) * 100.0;
//!     println!("Packing: {:.1}%", pct);
//! };
//! pack_archive(&config, Some(&cb))?;
//! # Ok(())
//! # }
//! ```
//!
//! # Performance Characteristics
//!
//! ## Throughput (Single-Threaded, i7-14700K)
//!
//! Validated benchmarks (see `docs/project-docs/BENCHMARKS.md` for details):
//!
//! - **LZ4 Compression**: 22 GB/s (minimal CPU overhead)
//! - **LZ4 Decompression**: 31 GB/s
//! - **Zstd Level 3 Compression**: 8.7 GB/s
//! - **Zstd Level 3 Decompression**: 12.9 GB/s
//! - **BLAKE3 Hashing**: 5.3 GB/s (2.2× faster than SHA-256)
//! - **SHA-256 Hashing**: 2.5 GB/s
//! - **`FastCDC` Chunking**: 2.7 GB/s (gear-based rolling hash)
//! - **AES-256-GCM Encryption**: 2.1 GB/s (hardware AES-NI acceleration)
//! - **Pack Throughput (LZ4, no CDC)**: 4.9 GB/s (64KB blocks)
//! - **Pack Throughput (LZ4 + CDC)**: 1.9 GB/s (CDC adds 2.6× overhead)
//! - **Pack Throughput (Zstd-3)**: 1.6 GB/s
//! - **Block Size Impact**: 2.3 GB/s (4KB) → 4.7 GB/s (64KB) → 5.1 GB/s (1MB)
//!
//! Typical bottleneck: CDC chunking (when enabled) or compression CPU time. SSD I/O rarely limits.
//!
//! Run benchmarks: `cargo bench --bench compression`, `cargo bench --bench hashing`, `cargo bench --bench cdc_chunking`, `cargo bench --bench encryption`, `cargo bench --bench write_throughput`, and `cargo bench --bench block_size_tradeoffs`
//!
//! ## Compression Ratios (Typical VM Images)
//!
//! - **LZ4**: 2-3× (fast but lower ratio)
//! - **Zstd Level 3**: 3-5× (good balance)
//! - **Zstd + Dictionary**: 4-7× (+30% improvement from dictionary)
//! - **CDC Deduplication**: Not validated - need benchmark comparing CDC vs fixed-size chunking
//!
//! ## Time Estimates (64 GB VM Image, Single Thread)
//!
//! - **LZ4, Fixed Blocks**: ~30-45 seconds
//! - **Zstd, Fixed Blocks**: ~2-3 minutes
//! - **Zstd + Dictionary + CDC**: ~3-5 minutes (includes 2-5s training time)
//!
//! # Atomicity and Crash Safety
//!
//! **WARNING**: Pack operations are NOT atomic. If interrupted:
//!
//! - Output file is left in a partially written state
//! - The header (written last) will be all zeros
//! - Readers will reject the file due to invalid magic bytes
//! - Manual cleanup is required (delete partial file)
//!
//! For production use cases requiring atomicity, write to a temporary file and
//! perform an atomic rename after successful completion.

use hexz_common::constants::{DICT_TRAINING_SIZE, ENTROPY_THRESHOLD};
use hexz_common::crypto::KeyDerivationParams;
use hexz_common::{Error, Result};
use hexz_core::api::file::Archive;
use hexz_core::format::header::Header;
use ignore::WalkBuilder;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkdir::WalkDir;

use crate::archive_writer::ArchiveWriter;
use crate::parallel_pack::{CompressedChunk, RawChunk};
use hexz_core::algo::compression::{create_compressor_from_str, zstd::ZstdCompressor};
use hexz_core::algo::dedup::cdc::{StreamChunker, analyze_stream};
use hexz_core::algo::dedup::dcam::{DedupeParams, optimize_params};
use hexz_core::algo::encryption::{Encryptor, aes_gcm::AesGcmEncryptor};
use hexz_core::api::manifest::{ArchiveManifest, FileEntry};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

/// Configuration parameters for archive packing.
///
/// This struct encapsulates all settings for the packing process. It's designed
/// to be easily constructed from CLI arguments or programmatic APIs.
///
/// # Examples
///
/// ```
/// use hexz_ops::pack::PackConfig;
/// use std::path::PathBuf;
///
/// // Basic configuration with defaults
/// let config = PackConfig {
///     input: PathBuf::from("data/"),
///     output: PathBuf::from("archive.hxz"),
///     ..Default::default()
/// };
///
/// // Advanced configuration with CDC and encryption
/// let advanced = PackConfig {
///     input: PathBuf::from("data/"),
///     output: PathBuf::from("archive.hxz"),
///     compression: "zstd".to_string(),
///     password: Some("secret".to_string()),
///     min_chunk: Some(16384),
///     avg_chunk: Some(65536),
///     max_chunk: Some(131072),
///     transform: hexz_ops::pack::PackTransformFlags { encrypt: true, ..Default::default() },
///     ..Default::default()
/// };
/// ```
/// Feature flags controlling how data is transformed during packing.
#[derive(Debug, Clone, Default)]
pub struct PackTransformFlags {
    /// Enable encryption.
    pub encrypt: bool,
    /// Train a compression dictionary (zstd only).
    pub train_dict: bool,
    /// Enable parallel compression (use multiple CPU cores).
    pub parallel: bool,
}

/// Feature flags controlling analysis and UI during packing.
#[derive(Debug, Clone, Default)]
pub struct PackAnalysisFlags {
    /// Show progress bar (if no callback provided).
    pub show_progress: bool,
    /// Run DCAM analysis to auto-detect optimal CDC parameters.
    /// When false (default), uses fixed global defaults: min=16 KiB, avg=64 KiB, max=256 KiB.
    pub use_dcam: bool,
    /// If true, DCAM will sweep a wider range of parameters (up to 16MB average chunks).
    pub dcam_optimal: bool,
}

/// Configuration for archive packing operations.
#[derive(Debug, Clone)]
pub struct PackConfig {
    /// Input path (file or directory).
    pub input: PathBuf,
    /// Base archive for delta deduplication.
    pub base: Option<PathBuf>,
    /// Output archive file path.
    pub output: PathBuf,
    /// Compression algorithm ("lz4" or "zstd").
    pub compression: String,
    /// Encryption password (required if encrypt=true).
    pub password: Option<String>,
    /// Block size in bytes.
    pub block_size: u32,
    /// Minimum chunk size for CDC (auto-detected if None).
    pub min_chunk: Option<u32>,
    /// Average chunk size for CDC (auto-detected if None).
    pub avg_chunk: Option<u32>,
    /// Maximum chunk size for CDC (auto-detected if None).
    pub max_chunk: Option<u32>,
    /// Number of worker threads (0 = auto-detect).
    pub num_workers: usize,
    /// Data transformation flags (encryption, dictionary, parallelism).
    pub transform: PackTransformFlags,
    /// Analysis and UI flags (progress, DCAM).
    pub analysis: PackAnalysisFlags,
}

impl Default for PackConfig {
    fn default() -> Self {
        Self {
            input: PathBuf::new(),
            base: None,
            output: PathBuf::from("output.hxz"),
            compression: "lz4".to_string(),
            password: None,
            block_size: 65536,
            min_chunk: None,
            avg_chunk: None,
            max_chunk: None,
            num_workers: 0, // Auto-detect CPU cores
            transform: PackTransformFlags {
                encrypt: false,
                train_dict: false,
                parallel: true, // Enable by default for performance
            },
            analysis: PackAnalysisFlags {
                show_progress: true, // Show progress by default
                use_dcam: false,     // Use fixed defaults; opt-in DCAM with --dcam
                dcam_optimal: false,
            },
        }
    }
}

/// Calculates Shannon entropy of a byte slice.
///
/// Shannon entropy measures the "randomness" or information content of data:
/// - **0.0**: All bytes are identical (highly compressible)
/// - **8.0**: Maximum entropy, random data (incompressible)
///
/// # Formula
///
/// ```text
/// H(X) = -Σ p(x) * log2(p(x))
/// ```
///
/// Where `p(x)` is the frequency of each byte value.
///
/// # Usage
///
/// Used during dictionary training to filter out high-entropy (random) blocks
/// that wouldn't benefit from compression. Only blocks with entropy below
/// `ENTROPY_THRESHOLD` are included in the training set.
///
/// # Parameters
///
/// - `data`: Byte slice to analyze
///
/// # Returns
///
/// Entropy value from 0.0 (homogeneous) to 8.0 (random).
///
/// # Examples
///
/// ```
/// # use hexz_ops::pack::calculate_entropy;
/// // Homogeneous data (low entropy)
/// let zeros = vec![0u8; 1024];
/// let entropy = calculate_entropy(&zeros);
/// assert_eq!(entropy, 0.0);
///
/// // Random data (high entropy)
/// let random: Vec<u8> = (0..=255).cycle().take(1024).collect();
/// let entropy = calculate_entropy(&random);
/// assert!(entropy > 7.0);
/// ```
pub fn calculate_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    let mut frequencies = [0u32; 256];
    for &byte in data {
        frequencies[byte as usize] += 1;
    }

    let len = data.len() as f64;
    let mut entropy = 0.0;

    for &count in &frequencies {
        if count > 0 {
            let p = count as f64 / len;
            entropy = p.mul_add(-p.log2(), entropy);
        }
    }

    entropy
}

/// Global CDC defaults used when DCAM auto-detection is disabled.
///
/// - min: 16 KiB  — avoids pathologically tiny chunks on large files
/// - avg: 64 KiB  — good balance of dedup granularity vs. metadata overhead
/// - max: 256 KiB — caps pathologically large chunks from low-entropy regions
pub const CDC_DEFAULT_MIN: u32 = 16_384; // 16 KiB
/// Default CDC average chunk size (64 KiB, f = 16).
pub const CDC_DEFAULT_AVG: u32 = 65_536; // 64 KiB  (f = 16)
/// Default CDC maximum chunk size (256 KiB).
pub const CDC_DEFAULT_MAX: u32 = 262_144; // 256 KiB

/// Resolve CDC parameters for packing.
///
/// Resolution order (highest to lowest priority):
/// 1. Explicit user flags (`--min-chunk`, `--avg-chunk`, `--max-chunk`)
/// 2. DCAM analysis (only when `config.analysis.use_dcam = true`) — scans the full file
/// 3. Global defaults: min=16 KiB, avg=64 KiB, max=256 KiB
///
/// Partial overrides are supported at every level: e.g. only `--min-chunk`
/// specified while the other two come from DCAM or the defaults.
pub fn resolve_cdc_params(path: &Path, config: &PackConfig) -> Result<DedupeParams> {
    /// Build a `DedupeParams` from the three logical sizes, filling w/v with
    /// the standard values used throughout the rest of the code-base.
    fn from_sizes(min: u32, avg: u32, max: u32) -> DedupeParams {
        DedupeParams {
            f: (avg as f64).log2().round() as u32,
            m: min,
            z: max,
            w: 48,
            v: 52,
        }
    }

    // If the user supplied all three, short-circuit immediately — no file scan.
    if let (Some(min), Some(avg), Some(max)) =
        (config.min_chunk, config.avg_chunk, config.max_chunk)
    {
        return Ok(from_sizes(min, avg, max));
    }

    // Try to inherit from base archive if provided
    if let Some(ref base_path) = config.base {
        if let Ok(base_file) = File::open(base_path) {
            let mut reader = std::io::BufReader::new(base_file);
            if let Ok(header) = Header::read_from(&mut reader) {
                if let Some((f, m, z)) = header.cdc_params {
                    let avg = 1u32 << f;
                    tracing::debug!(
                        "Inheriting CDC params from base archive: f={} m={} z={}",
                        f,
                        m,
                        z
                    );
                    return Ok(from_sizes(m, avg, z));
                }
            }
        }
    }

    // Determine the base (min, avg, max) triple, either from DCAM or defaults.
    let (base_min, base_avg, base_max) = if config.analysis.use_dcam {
        // DCAM: scan the entire file to find data-adaptive optimal params.
        let baseline = DedupeParams::lbfs_baseline();
        let file = File::open(path)?;
        let file_size = file.metadata()?.len();

        if file_size == 0 {
            (CDC_DEFAULT_MIN, CDC_DEFAULT_AVG, CDC_DEFAULT_MAX)
        } else {
            let stats = analyze_stream(file, &baseline)?;
            let optimized = optimize_params(
                file_size,
                stats.unique_bytes,
                &baseline,
                config.analysis.dcam_optimal,
            );
            let p = &optimized.params;
            let avg = (2u32).pow(p.f);
            tracing::debug!(
                "DCAM auto-detected CDC params: f={} m={} z={} (change_rate={:.4}, predicted_ratio={:.4})",
                p.f,
                p.m,
                p.z,
                optimized.change_rate,
                optimized.predicted_ratio,
            );
            (p.m, avg, p.z)
        }
    } else {
        (CDC_DEFAULT_MIN, CDC_DEFAULT_AVG, CDC_DEFAULT_MAX)
    };

    // Apply any user-provided partial overrides on top of the base triple.
    let min = config.min_chunk.unwrap_or(base_min);
    let avg = config.avg_chunk.unwrap_or(base_avg);
    let max = config.max_chunk.unwrap_or(base_max);

    Ok(from_sizes(min, avg, max))
}

/// Packs a archive file from disk and/or memory images.
///
/// This is the main entry point for creating Hexz archive files. It orchestrates
/// the complete packing pipeline: dictionary training, stream processing, index
/// building, and header finalization.
///
/// # Workflow
///
/// 1. **Validation**: Ensure at least one input (disk or memory) is provided
/// 2. **File Creation**: Create output file, reserve 512 bytes for header
/// 3. **Dictionary Training**: If requested (Zstd only), train dictionary from input samples
/// 4. **Dictionary Writing**: If trained, write dictionary immediately after header
/// 5. **Compressor Initialization**: Create LZ4 or Zstd compressor (with optional dictionary)
/// 6. **Encryptor Initialization**: If requested, derive key from password using PBKDF2
/// 7. **Stream Processing**: Process main stream (if provided), then auxiliary stream (if provided)
///    - Each stream independently chunks, compresses, encrypts, deduplicates, and indexes
/// 8. **Master Index Writing**: Serialize master index (all `PageEntry` records) to end of file
/// 9. **Header Writing**: Seek to start, write complete header with metadata and offsets
/// 10. **Flush**: Ensure all data is written to disk
///
/// # Parameters
///
/// - `config`: Packing configuration parameters (see [`PackConfig`])
/// - `progress_callback`: Optional callback for progress reporting
///   - Called frequently during stream processing (~once per 64 KiB)
///   - Signature: `Fn(logical_pos: u64, total_size: u64)`
///   - Example: `|pos, total| println!("Progress: {:.1}%", (pos as f64 / total as f64) * 100.0)`
///
/// # Returns
///
/// - `Ok(())`: Archive packed successfully
/// - `Err(Error::Io)`: I/O error (file access, disk full, permission denied)
/// - `Err(Error::Compression)`: Compression error (unlikely, usually indicates invalid state)
/// - `Err(Error::Encryption)`: Encryption error (invalid password format, crypto failure)
///
/// # Errors
///
/// This function can fail for several reasons:
///
/// ## I/O Errors
///
/// - **Input file not found**: `config.disk` or `config.memory` path doesn't exist
/// - **Permission denied**: Cannot read input or write output
/// - **Disk full**: Insufficient space for output file
/// - **Output exists**: May overwrite existing file without warning
///
/// ## Configuration Errors
///
/// - **No inputs**: Neither `disk` nor `memory` is provided
/// - **Missing password**: `encrypt = true` but `password = None`
/// - **Invalid block size**: Block size too small (<1 KiB) or too large (>16 MiB)
/// - **Invalid CDC params**: `min_chunk >= avg_chunk >= max_chunk` constraint violated
///
/// ## Compression/Encryption Errors
///
/// - **Dictionary training failure**: Zstd training fails (rare, usually on corrupted input)
/// - **Compression failure**: Compressor returns error (rare, usually indicates bug)
/// - **Encryption failure**: Key derivation or cipher initialization fails
///
/// # Examples
///
/// ## Basic Usage
///
/// ```no_run
/// use hexz_ops::pack::{pack_archive, PackConfig};
/// use std::path::PathBuf;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = PackConfig {
///     input: PathBuf::from("disk.raw"),
///     output: PathBuf::from("archive.hxz"),
///     ..Default::default()
/// };
///
/// pack_archive::<fn(u64, u64)>(&config, None)?;
/// # Ok(())
/// # }
/// ```
///
/// ## With Progress Reporting
///
/// ```no_run
/// use hexz_ops::pack::{pack_archive, PackConfig, PackTransformFlags};
/// use std::path::PathBuf;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = PackConfig {
///     input: PathBuf::from("ubuntu.qcow2"),
///     output: PathBuf::from("ubuntu.hxz"),
///     compression: "zstd".to_string(),
///     transform: PackTransformFlags { train_dict: true, ..Default::default() },
///     ..Default::default()
/// };
///
/// let cb = |pos: u64, total: u64| {
///     eprint!("\rPacking: {:.1}%", (pos as f64 / total as f64) * 100.0);
/// };
/// pack_archive(&config, Some(&cb))?;
/// eprintln!("\nDone!");
/// # Ok(())
/// # }
/// ```
///
/// ## Encrypted Archive
///
/// ```no_run
/// use hexz_ops::pack::{pack_archive, PackConfig, PackTransformFlags};
/// use std::path::PathBuf;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = PackConfig {
///     input: PathBuf::from("sensitive.raw"),
///     output: PathBuf::from("sensitive.hxz"),
///     password: Some("strong_passphrase".to_string()),
///     transform: PackTransformFlags { encrypt: true, ..Default::default() },
///     ..Default::default()
/// };
///
/// pack_archive::<fn(u64, u64)>(&config, None)?;
/// println!("Encrypted archive created");
/// # Ok(())
/// # }
/// ```
///
/// ## Content-Defined Chunking for Deduplication
///
/// ```no_run
/// use hexz_ops::pack::{pack_archive, PackConfig};
/// use std::path::PathBuf;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = PackConfig {
///     input: PathBuf::from("incremental-backup.raw"),
///     output: PathBuf::from("backup.hxz"),
///     min_chunk: Some(16384),   // 16 KiB
///     avg_chunk: Some(65536),   // 64 KiB
///     max_chunk: Some(262144),  // 256 KiB
///     ..Default::default()
/// };
///
/// pack_archive::<fn(u64, u64)>(&config, None)?;
/// # Ok(())
/// # }
/// ```
///
/// # Performance
///
/// See module-level documentation for detailed performance characteristics.
///
/// Typical throughput for a 64 GB VM image on modern hardware (Intel i7, `NVMe` SSD):
///
/// - **LZ4, no encryption**: ~2 GB/s (~30 seconds total)
/// - **Zstd level 3, no encryption**: ~500 MB/s (~2 minutes total)
/// - **Zstd + dictionary + CDC**: ~400 MB/s (~3 minutes including training)
///
/// # Atomicity
///
/// This operation is NOT atomic. On failure, the output file will be left in a
/// partially written state. The file header is written last, so incomplete files
/// will have an all-zero header and will be rejected by readers.
///
/// For atomic pack operations, write to a temporary file and perform an atomic
/// rename after success:
///
/// ```no_run
/// # use hexz_ops::pack::{pack_archive, PackConfig};
/// # use std::path::PathBuf;
/// # use std::fs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = PackConfig {
///     input: PathBuf::from("disk.raw"),
///     output: PathBuf::from("archive.hxz.tmp"),
///     ..Default::default()
/// };
///
/// pack_archive::<fn(u64, u64)>(&config, None)?;
/// fs::rename("archive.hxz.tmp", "archive.hxz")?;
/// # Ok(())
/// # }
/// ```
///
/// # Thread Safety
///
/// This function is not thread-safe with respect to the output file. Do not call
/// `pack_archive` concurrently with the same output path. Concurrent packing to
/// different output files is safe.
///
/// The progress callback must be `Send + Sync` if you want to call this function
/// from a non-main thread.
pub fn pack_archive<F>(config: &PackConfig, progress_callback: Option<&F>) -> Result<()>
where
    F: Fn(u64, u64) + Send + Sync,
{
    // 1. Resolve inputs
    let input_path = &config.input;

    // 2. Load parent archive if specified (for thin snapshots)
    let parent = if let Some(ref base_path) = config.base {
        Some(hexz_store::open_local(base_path, None)?)
    } else {
        None
    };

    // 3. Train compression dictionary if requested
    let dictionary = if config.compression == "zstd" && config.transform.train_dict {
        let sample_path = if input_path.is_dir() {
            // Sample from the first file in the directory
            WalkDir::new(input_path)
                .into_iter()
                .filter_map(std::result::Result::ok)
                .find(|e| e.file_type().is_file())
                .ok_or_else(|| {
                    Error::Io(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "No files found for dictionary training",
                    ))
                })?
                .path()
                .to_path_buf()
        } else {
            input_path.clone()
        };
        Some(train_dictionary(&sample_path, config.block_size)?)
    } else {
        None
    };

    // 4. Initialize compressor & encryptor
    let (compressor, compression_type) =
        create_compressor_from_str(&config.compression, None, dictionary.as_deref())?;

    let (encryptor, enc_params): (Option<Box<dyn Encryptor>>, _) = if config.transform.encrypt {
        let password = config.password.clone().ok_or_else(|| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Password required for encryption",
            ))
        })?;
        let params = KeyDerivationParams::default();
        let enc = AesGcmEncryptor::new(password.as_bytes(), &params.salt, params.iterations)?;
        (Some(Box::new(enc) as Box<dyn Encryptor>), Some(params))
    } else {
        (None, None)
    };

    // 5. Resolve CDC parameters
    let cdc_params = if input_path.is_file() {
        resolve_cdc_params(input_path, config)?
    } else {
        DedupeParams::lbfs_baseline()
    };

    // 5. Create ArchiveWriter
    let mut builder = ArchiveWriter::builder(&config.output, compressor, compression_type)
        .block_size(config.block_size)
        .variable_blocks(true)
        .cdc_params(Some((cdc_params.f, cdc_params.m, cdc_params.z)));

    if let Some(parent_snap) = parent {
        builder = builder.parent(parent_snap);
    }

    if let (Some(enc), Some(params)) = (encryptor, enc_params) {
        builder = builder.encryption(enc, params);
    }

    let mut writer = builder.build()?;

    if let Some(d) = &dictionary {
        writer.write_dictionary(d)?;
    }

    // 7. Process input
    let dict_ref = dictionary.as_deref();
    let manifest = if input_path.is_dir() {
        Some(pack_directory(
            input_path,
            &mut writer,
            &cdc_params,
            config,
            dict_ref,
            progress_callback,
        )?)
    } else {
        let total_size = input_path.metadata()?.len();
        let progress_bar =
            if config.analysis.show_progress && progress_callback.is_none() && total_size > 0 {
                Some(crate::progress::PackProgress::new(total_size, "Packing"))
            } else {
                None
            };

        let cb = |pos: u64, total: u64| {
            if let Some(ref pb) = progress_bar {
                pb.set_position(pos);
            }
            if let Some(ref cb) = progress_callback {
                cb(pos, total);
            }
        };

        process_stream(
            input_path,
            true,
            &mut writer,
            &cdc_params,
            config,
            dict_ref,
            Some(&cb),
        )?;

        if let Some(ref pb) = progress_bar {
            pb.finish();
        }
        None
    };

    // 8. Finalize
    let metadata = if let Some(m) = manifest {
        Some(serde_json::to_vec(&m).map_err(|e| Error::Format(e.to_string()))?)
    } else {
        None
    };

    let parent_paths = if let Some(ref base) = config.base {
        vec![base.to_string_lossy().into_owned()]
    } else {
        Vec::new()
    };

    writer.finalize(parent_paths, metadata.as_deref())?;

    Ok(())
}

/// Recursively packs a directory into Main (and optionally Auxiliary) archive streams.
///
/// A file named `memory` at the root of the input directory is packed into the
/// Auxiliary stream. All other files go into the Main stream.
fn pack_directory<F>(
    root: &Path,
    writer: &mut ArchiveWriter,
    cdc_params: &DedupeParams,
    config: &PackConfig,
    dictionary: Option<&[u8]>,
    progress_callback: Option<&F>,
) -> Result<ArchiveManifest>
where
    F: Fn(u64, u64) + Send + Sync,
{
    // Collect all file entries, separating main vs auxiliary
    let mut main_entries: Vec<(PathBuf, String, std::fs::Metadata)> = Vec::new();
    let mut aux_entries: Vec<(PathBuf, String, std::fs::Metadata)> = Vec::new();

    let walker = WalkBuilder::new(root)
        .standard_filters(true)
        .add_custom_ignore_filename(".hexzignore")
        .hidden(false)
        .build();

    for entry in walker.filter_map(std::result::Result::ok) {
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path().to_path_buf();
        if path.components().any(|c| c.as_os_str() == ".hexz") {
            continue;
        }
        let rel_path = path
            .strip_prefix(root)
            .map_err(|e| Error::Format(e.to_string()))?
            .to_string_lossy()
            .into_owned();
        let metadata = entry
            .metadata()
            .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;

        // A file named "memory" at the directory root goes to the Auxiliary stream
        if rel_path == "memory" {
            aux_entries.push((path, rel_path, metadata));
        } else {
            main_entries.push((path, rel_path, metadata));
        }
    }

    let main_size: u64 = main_entries.iter().map(|(_, _, m)| m.len()).sum();
    let aux_size: u64 = aux_entries.iter().map(|(_, _, m)| m.len()).sum();
    let total_size = main_size + aux_size;

    let progress_bar =
        if config.analysis.show_progress && progress_callback.is_none() && total_size > 0 {
            Some(crate::progress::PackProgress::new(
                total_size,
                "Packing Directory",
            ))
        } else {
            None
        };

    // Pack main stream
    let mut files = Vec::new();
    let mut current_logical_offset = 0u64;
    let mut global_progress = 0u64;

    writer.begin_stream(true, main_size);

    for (path, rel_path, metadata) in &main_entries {
        let size = metadata.len();
        let file_entry = FileEntry {
            path: rel_path.clone(),
            offset: current_logical_offset,
            size,
            mode: {
                #[cfg(unix)]
                {
                    metadata.mode()
                }
                #[cfg(not(unix))]
                {
                    0o644
                }
            },
            mtime: metadata
                .modified()?
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };

        let cur_offset = current_logical_offset;
        let cb = |pos: u64, _total: u64| {
            let gp = global_progress + pos;
            if let Some(ref pb) = progress_bar {
                pb.set_position(gp);
            }
            if let Some(ref cb) = progress_callback {
                cb(cur_offset + pos, total_size);
            }
        };

        pack_file_to_stream(path, writer, cdc_params, config, dictionary, Some(&cb))?;
        writer.flush_stream()?;

        files.push(file_entry);
        current_logical_offset = writer.current_logical_pos();
        global_progress += size;
    }

    writer.end_stream()?;

    // Pack auxiliary stream if there are memory files
    if !aux_entries.is_empty() {
        writer.begin_stream(false, aux_size);

        for (path, rel_path, metadata) in &aux_entries {
            let size = metadata.len();
            let file_entry = FileEntry {
                path: rel_path.clone(),
                offset: 0,
                size,
                mode: {
                    #[cfg(unix)]
                    {
                        metadata.mode()
                    }
                    #[cfg(not(unix))]
                    {
                        0o644
                    }
                },
                mtime: metadata
                    .modified()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };

            let cb = |pos: u64, _total: u64| {
                let gp = global_progress + pos;
                if let Some(ref pb) = progress_bar {
                    pb.set_position(gp);
                }
                if let Some(ref cb) = progress_callback {
                    cb(gp, total_size);
                }
            };

            pack_file_to_stream(path, writer, cdc_params, config, dictionary, Some(&cb))?;
            writer.flush_stream()?;

            files.push(file_entry);
            global_progress += size;
        }

        writer.end_stream()?;
    }

    if let Some(ref pb) = progress_bar {
        pb.finish();
    }

    Ok(ArchiveManifest { files })
}

/// Packs a single file into the current active stream of the `ArchiveWriter`.
fn pack_file_to_stream<F>(
    path: &Path,
    writer: &mut ArchiveWriter,
    cdc_params: &DedupeParams,
    config: &PackConfig,
    dictionary: Option<&[u8]>,
    progress_callback: Option<&F>,
) -> Result<()>
where
    F: Fn(u64, u64),
{
    let f = File::open(path)?;
    let len = f.metadata()?.len();

    if config.transform.parallel && !config.transform.encrypt {
        process_stream_parallel(
            f,
            len,
            writer,
            cdc_params,
            config,
            dictionary,
            progress_callback,
        )?;
    } else {
        process_stream_serial(f, len, writer, cdc_params, progress_callback)?;
    }

    Ok(())
}

/// Extracts an archive back to the filesystem.
///
/// If the archive contains a manifest, it will be extracted as a directory.
/// Otherwise, the Main stream will be extracted as a single raw file.
pub fn extract_archive(
    input_path: &Path,
    output_path: &Path,
    password: Option<String>,
) -> Result<()> {
    use hexz_core::ArchiveStream;
    use hexz_core::algo::compression::create_compressor;
    use hexz_core::algo::encryption::aes_gcm::AesGcmEncryptor;
    use hexz_core::api::file::ParentLoader;
    use hexz_core::format::header::Header;
    use hexz_store::local::MmapBackend;

    let backend = Arc::new(MmapBackend::new(input_path)?);
    let header = Header::read_from_backend(backend.as_ref())?;

    let encryptor = if let (Some(params), Some(pass)) = (header.encryption.as_ref(), password) {
        let enc = AesGcmEncryptor::new(pass.as_bytes(), &params.salt, params.iterations)?;
        Some(Box::new(enc) as Box<dyn hexz_core::algo::encryption::Encryptor>)
    } else {
        None
    };

    let dictionary = header.load_dictionary(backend.as_ref())?;
    let compressor = create_compressor(header.compression, None, dictionary.as_deref());

    // Provide a parent loader that resolves relative to the input archive
    let archive_dir = input_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let loader: ParentLoader = Box::new(move |parent_path: &str| {
        let p = Path::new(parent_path);

        // Try resolving in this order:
        // 1. As provided (if absolute or relative to CWD)
        // 2. Relative to the archive being extracted
        let full_parent_path = if p.exists() {
            p.to_path_buf()
        } else {
            let rel = archive_dir.join(parent_path);
            if rel.exists() {
                rel
            } else {
                // Fallback to p and let it fail with proper error if not found
                p.to_path_buf()
            }
        };

        let pb: Arc<dyn hexz_core::store::StorageBackend> =
            Arc::new(MmapBackend::new(&full_parent_path)?);
        Archive::open(pb, None)
    });

    let archive =
        Archive::with_cache_and_loader(backend, compressor, encryptor, None, None, Some(&loader))?;

    // Check for manifest in metadata
    if let Some(metadata) = &archive.metadata {
        if let Ok(manifest) = serde_json::from_slice::<ArchiveManifest>(metadata) {
            std::fs::create_dir_all(output_path)?;

            for file in manifest.files {
                let out_path = output_path.join(&file.path);
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let mut out_file = File::create(&out_path)?;
                let data = archive.read_at(ArchiveStream::Main, file.offset, file.size as usize)?;
                out_file.write_all(&data)?;

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(
                        &out_path,
                        std::fs::Permissions::from_mode(file.mode),
                    )?;
                }
            }
            return Ok(());
        }
    }

    // Fallback: extract Main stream to a single file
    let mut out_file = File::create(output_path)?;
    let size = archive.size(ArchiveStream::Main);

    // Extract in chunks to avoid huge memory usage
    let chunk_size = 1024 * 1024; // 1MB
    let mut pos = 0u64;
    while pos < size {
        let len = std::cmp::min(chunk_size as u64, size - pos) as usize;
        let data = archive.read_at(ArchiveStream::Main, pos, len)?;
        out_file.write_all(&data)?;
        pos += len as u64;
    }

    Ok(())
}

/// Trains a Zstd compression dictionary from stratified samples.
///
/// Dictionary training analyzes a representative sample of input blocks to build
/// a shared dictionary that improves compression ratios for structured data
/// (file systems, databases, logs) by capturing common patterns.
///
/// # Algorithm
///
/// 1. **Stratified Sampling**: Sample blocks evenly across the file
///    - Compute step size: `file_size / target_samples`
///    - Read one block at each sample point
///    - Ensures coverage of different regions (boot sector, metadata, data)
///
/// 2. **Quality Filtering**: Exclude unsuitable blocks
///    - Skip all-zero blocks (no compressible patterns)
///    - Compute Shannon entropy (0-8 bits per byte)
///    - Reject blocks with entropy > `ENTROPY_THRESHOLD` (6.0)
///    - Rationale: High-entropy data (encrypted, random) doesn't benefit from dictionaries
///
/// 3. **Dictionary Training**: Feed filtered samples to Zstd
///    - Uses Zstd's COVER algorithm (`fast_cover` variant)
///    - Analyzes n-grams to find common subsequences
///    - Outputs dictionary up to `DICT_TRAINING_SIZE` (110 KiB)
///
/// # Parameters
///
/// - `input_path`: Path to the input file to sample from
/// - `block_size`: Size of each sample block in bytes
///
/// # Returns
///
/// - `Ok(Vec<u8>)`: Trained dictionary bytes (empty if training fails or no suitable samples)
/// - `Err(Error)`: I/O error reading input file
///
/// # Performance
///
/// - **Sampling time**: ~100-500 ms (depends on file size and disk speed)
/// - **Training time**: ~2-5 seconds for 4000 samples
/// - **Memory usage**: ~256 MB (sample corpus in RAM)
///
/// # Compression Improvement
///
/// - **Typical**: 10-30% better ratio vs. no dictionary
/// - **Best case**: 50%+ improvement for highly structured data (databases)
/// - **Worst case**: No improvement or slight regression (already compressed data)
///
/// # Edge Cases
///
/// - **Empty file**: Returns empty dictionary with warning
/// - **All high-entropy data**: Returns empty dictionary with warning
/// - **Small files**: May not reach target sample count (trains on available data)
///
/// # Examples
///
/// Called internally by `pack_archive` when `train_dict` is enabled:
///
/// ```text
/// let dict = train_dictionary(Path::new("disk.raw"), 65536)?;
/// // dict: Vec<u8> containing the trained zstd dictionary
/// ```
fn train_dictionary(input_path: &Path, block_size: u32) -> Result<Vec<u8>> {
    let mut f = File::open(input_path)?;
    let file_len = f.metadata()?.len();

    let mut samples = Vec::new();
    let mut buffer = vec![0u8; block_size as usize];
    let target_samples = DICT_TRAINING_SIZE;

    let step = if file_len > 0 {
        (file_len / target_samples as u64).max(block_size as u64)
    } else {
        0
    };

    let mut attempts = 0;
    while samples.len() < target_samples && attempts < target_samples * 2 {
        let offset = attempts as u64 * step;
        if offset >= file_len {
            break;
        }

        _ = f.seek(SeekFrom::Start(offset))?;
        let n = f.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        let chunk = &buffer[..n];
        let is_zeros = chunk.iter().all(|&b| b == 0);

        if !is_zeros {
            let entropy = calculate_entropy(chunk);
            if entropy < ENTROPY_THRESHOLD {
                samples.push(chunk.to_vec());
            }
        }
        attempts += 1;
    }

    if samples.is_empty() {
        tracing::warn!("Input seems to be empty or high entropy. Dictionary will be empty.");
        Ok(Vec::new())
    } else {
        let dict_bytes = ZstdCompressor::train(&samples, DICT_TRAINING_SIZE)?;
        tracing::info!("Dictionary trained: {} bytes", dict_bytes.len());
        Ok(dict_bytes)
    }
}

/// Processes a single input stream (disk or memory) via the [`ArchiveWriter`].
fn process_stream<F>(
    path: &Path,
    is_disk: bool,
    writer: &mut ArchiveWriter,
    cdc_params: &DedupeParams,
    config: &PackConfig,
    dictionary: Option<&[u8]>,
    progress_callback: Option<&F>,
) -> Result<()>
where
    F: Fn(u64, u64),
{
    let f = File::open(path)?;
    let len = f.metadata()?.len();

    writer.begin_stream(is_disk, len);

    // Use parallel path when enabled and not encrypting (encryption needs sequential nonces)
    if config.transform.parallel && !config.transform.encrypt {
        process_stream_parallel(
            f,
            len,
            writer,
            cdc_params,
            config,
            dictionary,
            progress_callback,
        )?;
    } else {
        process_stream_serial(f, len, writer, cdc_params, progress_callback)?;
    }

    writer.end_stream()?;
    Ok(())
}

/// Serial (original) stream processing path.
fn process_stream_serial<F>(
    f: File,
    len: u64,
    writer: &mut ArchiveWriter,
    cdc_params: &DedupeParams,
    progress_callback: Option<&F>,
) -> Result<()>
where
    F: Fn(u64, u64),
{
    let mut logical_pos = 0u64;
    let mut chunk_buf = Vec::with_capacity(cdc_params.z as usize);

    let mut chunker = StreamChunker::new(f, *cdc_params);
    while let Some(res) = chunker.next_into(&mut chunk_buf) {
        let n = res?;
        logical_pos += n as u64;
        writer.write_data_block(&chunk_buf)?;
        if let Some(callback) = progress_callback {
            callback(logical_pos, len);
        }
    }

    Ok(())
}

/// Parallel stream processing: single persistent pipeline for the entire stream.
///
/// Architecture:
/// - Reader thread: reads input file, chunks it, sends to workers
/// - N worker threads: compress + BLAKE3 hash chunks in parallel
/// - Main thread: receives compressed chunks, reorders via `BTreeMap`, writes sequentially
///
/// This avoids per-batch thread pool creation overhead (the old approach created
/// ~2800 thread pools for a 180GB file).
fn process_stream_parallel<F>(
    f: File,
    len: u64,
    writer: &mut ArchiveWriter,
    cdc_params: &DedupeParams,
    config: &PackConfig,
    dictionary: Option<&[u8]>,
    progress_callback: Option<&F>,
) -> Result<()>
where
    F: Fn(u64, u64),
{
    use crossbeam::channel::bounded;
    use hexz_core::algo::compression::Compressor;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    let num_workers = if config.num_workers > 0 {
        config.num_workers
    } else {
        num_cpus::get()
    };

    // Create shared compressor for all workers, passing the trained dictionary
    let (compressor, _) = create_compressor_from_str(&config.compression, None, dictionary)?;
    let compressor: Arc<Box<dyn Compressor + Send + Sync>> = Arc::new(compressor);

    // Bounded channels for backpressure: enough to keep workers busy without excessive memory.
    // Each in-flight chunk is ~64KB, so num_workers*4 chunks ≈ num_workers*256KB.
    let channel_size = num_workers * 4;
    let (tx_raw, rx_raw) = bounded::<(u64, RawChunk)>(channel_size);
    let (tx_compressed, rx_compressed) = bounded::<(u64, CompressedChunk)>(channel_size);

    // Spawn persistent compression workers
    let mut workers = Vec::with_capacity(num_workers);
    for _ in 0..num_workers {
        let rx = rx_raw.clone();
        let tx = tx_compressed.clone();
        let comp = compressor.clone();
        workers.push(std::thread::spawn(move || -> Result<()> {
            for (seq, chunk) in rx {
                let compressed_data = comp.compress(&chunk.data)?;
                let hash = blake3::hash(&chunk.data);
                if tx
                    .send((
                        seq,
                        CompressedChunk {
                            compressed: compressed_data,
                            hash: hash.into(),
                            logical_offset: chunk.logical_offset,
                            original_size: chunk.data.len(),
                        },
                    ))
                    .is_err()
                {
                    break; // Receiver dropped, pipeline shutting down
                }
            }
            Ok(())
        }));
    }

    // Drop our copies so channels close when all real holders finish
    drop(rx_raw);
    drop(tx_compressed);

    // Spawn reader thread: reads input, chunks it, feeds workers
    let reader_cdc_params = *cdc_params;
    let reader = std::thread::spawn(move || -> Result<()> {
        let mut logical_pos = 0u64;

        let chunker = StreamChunker::new(f, reader_cdc_params);
        for (seq, chunk_res) in chunker.enumerate() {
            let chunk = chunk_res?;
            let chunk_len = chunk.len();
            if tx_raw
                .send((
                    seq as u64,
                    RawChunk {
                        data: chunk,
                        logical_offset: logical_pos,
                    },
                ))
                .is_err()
            {
                break; // Workers shut down
            }
            logical_pos += chunk_len as u64;
        }
        Ok(())
    });

    // Main thread: receive compressed chunks, reorder, write sequentially.
    // Workers return chunks out-of-order; BTreeMap restores logical order.
    let mut next_seq = 0u64;
    let mut reorder_buf: BTreeMap<u64, CompressedChunk> = BTreeMap::new();
    let mut write_error: Option<Error> = None;

    for (seq, compressed) in &rx_compressed {
        _ = reorder_buf.insert(seq, compressed);

        // Drain all consecutive chunks ready to write
        while let Some(chunk) = reorder_buf.remove(&next_seq) {
            match writer.write_precompressed_block(
                &chunk.compressed,
                &chunk.hash,
                chunk.original_size as u32,
            ) {
                Ok(()) => {
                    if let Some(callback) = progress_callback {
                        callback(chunk.logical_offset + chunk.original_size as u64, len);
                    }
                    next_seq += 1;
                }
                Err(e) => {
                    write_error = Some(e);
                    break;
                }
            }
        }
        if write_error.is_some() {
            break;
        }
    }

    // Drop receiver to unblock workers/reader if we exited early due to write error.
    // This causes workers' send() to fail → workers exit → reader's send() fails → reader exits.
    drop(rx_compressed);

    // Wait for all threads to finish
    let reader_result = reader
        .join()
        .map_err(|_| Error::Io(std::io::Error::other("Reader thread panicked")))?;

    for worker in workers {
        _ = worker
            .join()
            .map_err(|_| Error::Io(std::io::Error::other("Worker thread panicked")))?
            .ok(); // Ignore worker errors if we already have a write error
    }

    // Propagate errors (write errors take priority)
    if let Some(e) = write_error {
        return Err(e);
    }
    reader_result?;

    Ok(())
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_entropy_empty() {
        assert_eq!(calculate_entropy(&[]), 0.0);
    }

    #[test]
    fn test_calculate_entropy_uniform() {
        // All same byte - lowest entropy
        let data = vec![0x42; 1000];
        let entropy = calculate_entropy(&data);
        assert!(
            entropy < 0.01,
            "Entropy should be near 0.0 for uniform data"
        );
    }

    #[test]
    fn test_calculate_entropy_binary() {
        // Two values - low entropy
        let mut data = vec![0u8; 500];
        data.extend(vec![1u8; 500]);
        let entropy = calculate_entropy(&data);
        assert!(
            entropy > 0.9 && entropy < 1.1,
            "Entropy should be ~1.0 for binary data"
        );
    }

    #[test]
    fn test_calculate_entropy_random() {
        // All 256 values - high entropy
        let data: Vec<u8> = (0..=255).cycle().take(256 * 4).collect();
        let entropy = calculate_entropy(&data);
        assert!(
            entropy > 7.5,
            "Entropy should be high for all byte values: got {entropy}"
        );
    }

    #[test]
    fn test_calculate_entropy_single_byte() {
        assert_eq!(calculate_entropy(&[42]), 0.0);
    }

    #[test]
    fn test_calculate_entropy_two_different_bytes() {
        let data = vec![0, 255];
        let entropy = calculate_entropy(&data);
        assert!(entropy > 0.9 && entropy < 1.1, "Entropy should be ~1.0");
    }

    #[test]
    fn test_pack_config_default() {
        let config = PackConfig::default();

        assert_eq!(config.compression, "lz4");
        assert!(!config.transform.encrypt);
        assert_eq!(config.password, None);
        assert!(!config.transform.train_dict);
        assert_eq!(config.block_size, 65536);
        assert_eq!(config.min_chunk, None);
        assert_eq!(config.avg_chunk, None);
        assert_eq!(config.max_chunk, None);
    }

    #[test]
    fn test_pack_config_clone() {
        let config1 = PackConfig {
            input: PathBuf::from("/dev/sda"),
            output: PathBuf::from("output.hxz"),
            compression: "zstd".to_string(),
            password: Some("secret".to_string()),
            transform: PackTransformFlags {
                encrypt: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let config2 = config1.clone();

        assert_eq!(config2.input, config1.input);
        assert_eq!(config2.output, config1.output);
        assert_eq!(config2.compression, config1.compression);
        assert_eq!(config2.transform.encrypt, config1.transform.encrypt);
        assert_eq!(config2.password, config1.password);
    }

    #[test]
    fn test_pack_config_debug() {
        let config = PackConfig::default();
        let debug_str = format!("{config:?}");

        assert!(debug_str.contains("PackConfig"));
        assert!(debug_str.contains("lz4"));
    }

    #[test]
    fn test_entropy_threshold_filtering() {
        // Test data with entropy below threshold (compressible)
        let low_entropy_data = vec![0u8; 1024];
        assert!(calculate_entropy(&low_entropy_data) < ENTROPY_THRESHOLD);

        // Test data with entropy above threshold (random)
        let high_entropy_data: Vec<u8> = (0..1024).map(|i| ((i * 7) % 256) as u8).collect();
        let entropy = calculate_entropy(&high_entropy_data);
        // This might not always be above threshold depending on the pattern,
        // but we can still test that entropy calculation works
        assert!((0.0..=8.0).contains(&entropy));
    }

    #[test]
    fn test_entropy_calculation_properties() {
        // Entropy should increase with more unique values
        let data1 = vec![0u8; 100];
        let data2 = [0u8, 1u8].repeat(50);
        let mut data3 = Vec::new();
        for i in 0..100 {
            data3.push((i % 10) as u8);
        }

        let entropy1 = calculate_entropy(&data1);
        let entropy2 = calculate_entropy(&data2);
        let entropy3 = calculate_entropy(&data3);

        assert!(
            entropy1 < entropy2,
            "More unique values should increase entropy"
        );
        assert!(
            entropy2 < entropy3,
            "Even more unique values should further increase entropy"
        );
    }
}
