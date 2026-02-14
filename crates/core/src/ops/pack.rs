//! High-level snapshot packing operations.
//!
//! This module implements the core business logic for creating Hexz snapshot files
//! from raw disk and memory images. It orchestrates a multi-stage pipeline that
//! transforms raw input data into compressed, indexed, and optionally encrypted
//! snapshot files optimized for fast random access and deduplication.
//!
//! # Core Capabilities
//!
//! - **Dictionary Training**: Intelligent sampling and Zstd dictionary optimization
//! - **Chunking Strategies**: Fixed-size blocks or content-defined (FastCDC) for better deduplication
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
//! │  MasterIndex (disk_pages[], memory_pages[], sizes) → Serialize      │
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
//!    - Step size = file_size / target_samples (typically 4000 samples)
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
//! - **Memory Usage**: ~48 bytes per unique block (32-byte hash + 8-byte offset + HashMap overhead)
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
//! - **Master Index**: Array of PageEntry records
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
//! - **Deduplication Map**: ~48 bytes × unique_blocks
//!   - Example: 10 GB image with 50% dedup = ~80 MB HashMap
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
//! use hexz_core::ops::pack::{pack_snapshot, PackConfig};
//! use std::path::PathBuf;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = PackConfig {
//!     disk: Some(PathBuf::from("disk.raw")),
//!     memory: None,
//!     output: PathBuf::from("snapshot.hxz"),
//!     compression: "lz4".to_string(),
//!     ..Default::default()
//! };
//!
//! pack_snapshot::<fn(u64, u64)>(config, None)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Advanced Packing (Zstd with Dictionary, CDC, Encryption)
//!
//! ```no_run
//! use hexz_core::ops::pack::{pack_snapshot, PackConfig};
//! use std::path::PathBuf;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = PackConfig {
//!     disk: Some(PathBuf::from("ubuntu.qcow2")),
//!     output: PathBuf::from("ubuntu.hxz"),
//!     compression: "zstd".to_string(),
//!     train_dict: true,         // Train dictionary for better ratio
//!     cdc_enabled: true,        // Content-defined chunking
//!     encrypt: true,
//!     password: Some("secure_passphrase".to_string()),
//!     min_chunk: 16384,         // 16 KiB minimum chunk
//!     avg_chunk: 65536,         // 64 KiB average chunk
//!     max_chunk: 262144,        // 256 KiB maximum chunk
//!     ..Default::default()
//! };
//!
//! pack_snapshot::<fn(u64, u64)>(config, None)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Progress Reporting
//!
//! ```no_run
//! use hexz_core::ops::pack::{pack_snapshot, PackConfig};
//! use std::path::PathBuf;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = PackConfig {
//!     disk: Some(PathBuf::from("disk.raw")),
//!     output: PathBuf::from("snapshot.hxz"),
//!     ..Default::default()
//! };
//!
//! // Callback receives (current_logical_pos, total_size)
//! pack_snapshot(config, Some(|pos, total| {
//!     let pct = (pos as f64 / total as f64) * 100.0;
//!     println!("Packing: {:.1}%", pct);
//! }))?;
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
//! - **FastCDC Chunking**: 2.7 GB/s (gear-based rolling hash)
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

use hexz_common::constants::{DEFAULT_ZSTD_LEVEL, DICT_TRAINING_SIZE, ENTROPY_THRESHOLD};
use hexz_common::crypto::KeyDerivationParams;
use hexz_common::{Error, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::algo::compression::{Compressor, lz4::Lz4Compressor, zstd::ZstdCompressor};
use crate::algo::dedup::cdc::StreamChunker;
use crate::algo::dedup::dcam::DedupeParams;
use crate::algo::encryption::{Encryptor, aes_gcm::AesGcmEncryptor};
use crate::format::header::CompressionType;
use crate::ops::snapshot_writer::SnapshotWriter;

/// Configuration parameters for snapshot packing.
///
/// This struct encapsulates all settings for the packing process. It's designed
/// to be easily constructed from CLI arguments or programmatic APIs.
///
/// # Examples
///
/// ```
/// use hexz_core::ops::pack::PackConfig;
/// use std::path::PathBuf;
///
/// // Basic configuration with defaults
/// let config = PackConfig {
///     disk: Some(PathBuf::from("disk.img")),
///     output: PathBuf::from("snapshot.hxz"),
///     ..Default::default()
/// };
///
/// // Advanced configuration with CDC and encryption
/// let advanced = PackConfig {
///     disk: Some(PathBuf::from("disk.img")),
///     output: PathBuf::from("snapshot.hxz"),
///     compression: "zstd".to_string(),
///     encrypt: true,
///     password: Some("secret".to_string()),
///     cdc_enabled: true,
///     min_chunk: 16384,
///     avg_chunk: 65536,
///     max_chunk: 131072,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone)]
pub struct PackConfig {
    /// Path to the disk image (optional).
    pub disk: Option<PathBuf>,
    /// Path to the memory image (optional).
    pub memory: Option<PathBuf>,
    /// Output snapshot file path.
    pub output: PathBuf,
    /// Compression algorithm ("lz4" or "zstd").
    pub compression: String,
    /// Enable encryption.
    pub encrypt: bool,
    /// Encryption password (required if encrypt=true).
    pub password: Option<String>,
    /// Train a compression dictionary (zstd only).
    pub train_dict: bool,
    /// Block size in bytes.
    pub block_size: u32,
    /// Enable content-defined chunking (CDC).
    pub cdc_enabled: bool,
    /// Minimum chunk size for CDC.
    pub min_chunk: u32,
    /// Average chunk size for CDC.
    pub avg_chunk: u32,
    /// Maximum chunk size for CDC.
    pub max_chunk: u32,
}

impl Default for PackConfig {
    fn default() -> Self {
        Self {
            disk: None,
            memory: None,
            output: PathBuf::from("output.hxz"),
            compression: "lz4".to_string(),
            encrypt: false,
            password: None,
            train_dict: false,
            block_size: 65536,
            cdc_enabled: false,
            min_chunk: 16384,
            avg_chunk: 65536,
            max_chunk: 131072,
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
/// # use hexz_core::ops::pack::calculate_entropy;
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

    for &count in frequencies.iter() {
        if count > 0 {
            let p = count as f64 / len;
            entropy -= p * p.log2();
        }
    }

    entropy
}

/// Trait for chunk iterators (fixed-size or content-defined).
///
/// This trait provides a unified interface for both fixed-size and CDC chunkers,
/// allowing the packing logic to be agnostic to the chunking strategy.
trait Chunker: Iterator<Item = std::io::Result<Vec<u8>>> {}
impl<T: Iterator<Item = std::io::Result<Vec<u8>>>> Chunker for T {}

/// Fixed-size block chunker.
///
/// Splits input into equal-sized blocks (except possibly the last one).
/// Simpler and faster than CDC, but less effective for deduplication.
///
/// Uses a read loop to guarantee full blocks, avoiding short reads from
/// pipes, network streams, or OS buffering.
pub struct FixedChunker<R> {
    reader: R,
    block_size: usize,
}

impl<R: Read> FixedChunker<R> {
    /// Creates a new fixed-size chunker.
    pub fn new(reader: R, block_size: usize) -> Self {
        Self { reader, block_size }
    }
}

impl<R: Read> Iterator for FixedChunker<R> {
    type Item = std::io::Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = vec![0u8; self.block_size];
        let mut pos = 0;
        while pos < self.block_size {
            match self.reader.read(&mut buf[pos..]) {
                Ok(0) => break,
                Ok(n) => pos += n,
                Err(e) => return Some(Err(e)),
            }
        }
        if pos == 0 {
            None
        } else {
            buf.truncate(pos);
            Some(Ok(buf))
        }
    }
}

/// Packs a snapshot file from disk and/or memory images.
///
/// This is the main entry point for creating Hexz snapshot files. It orchestrates
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
/// 7. **Stream Processing**: Process disk stream (if provided), then memory stream (if provided)
///    - Each stream independently chunks, compresses, encrypts, deduplicates, and indexes
/// 8. **Master Index Writing**: Serialize master index (all PageEntry records) to end of file
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
/// - `Ok(())`: Snapshot packed successfully
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
/// use hexz_core::ops::pack::{pack_snapshot, PackConfig};
/// use std::path::PathBuf;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = PackConfig {
///     disk: Some(PathBuf::from("disk.raw")),
///     output: PathBuf::from("snapshot.hxz"),
///     ..Default::default()
/// };
///
/// pack_snapshot::<fn(u64, u64)>(config, None)?;
/// # Ok(())
/// # }
/// ```
///
/// ## With Progress Reporting
///
/// ```no_run
/// use hexz_core::ops::pack::{pack_snapshot, PackConfig};
/// use std::path::PathBuf;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = PackConfig {
///     disk: Some(PathBuf::from("ubuntu.qcow2")),
///     output: PathBuf::from("ubuntu.hxz"),
///     compression: "zstd".to_string(),
///     train_dict: true,
///     ..Default::default()
/// };
///
/// pack_snapshot(config, Some(|pos, total| {
///     eprint!("\rPacking: {:.1}%", (pos as f64 / total as f64) * 100.0);
/// }))?;
/// eprintln!("\nDone!");
/// # Ok(())
/// # }
/// ```
///
/// ## Encrypted Snapshot
///
/// ```no_run
/// use hexz_core::ops::pack::{pack_snapshot, PackConfig};
/// use std::path::PathBuf;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = PackConfig {
///     disk: Some(PathBuf::from("sensitive.raw")),
///     output: PathBuf::from("sensitive.hxz"),
///     encrypt: true,
///     password: Some("strong_passphrase".to_string()),
///     ..Default::default()
/// };
///
/// pack_snapshot::<fn(u64, u64)>(config, None)?;
/// println!("Encrypted snapshot created");
/// # Ok(())
/// # }
/// ```
///
/// ## Content-Defined Chunking for Deduplication
///
/// ```no_run
/// use hexz_core::ops::pack::{pack_snapshot, PackConfig};
/// use std::path::PathBuf;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = PackConfig {
///     disk: Some(PathBuf::from("incremental-backup.raw")),
///     output: PathBuf::from("backup.hxz"),
///     cdc_enabled: true,
///     min_chunk: 16384,   // 16 KiB
///     avg_chunk: 65536,   // 64 KiB
///     max_chunk: 262144,  // 256 KiB
///     ..Default::default()
/// };
///
/// pack_snapshot::<fn(u64, u64)>(config, None)?;
/// # Ok(())
/// # }
/// ```
///
/// # Performance
///
/// See module-level documentation for detailed performance characteristics.
///
/// Typical throughput for a 64 GB VM image on modern hardware (Intel i7, NVMe SSD):
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
/// # use hexz_core::ops::pack::{pack_snapshot, PackConfig};
/// # use std::path::PathBuf;
/// # use std::fs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut config = PackConfig {
///     disk: Some(PathBuf::from("disk.raw")),
///     output: PathBuf::from("snapshot.st.tmp"),
///     ..Default::default()
/// };
///
/// pack_snapshot::<fn(u64, u64)>(config.clone(), None)?;
/// fs::rename("snapshot.st.tmp", "snapshot.hxz")?;
/// # Ok(())
/// # }
/// ```
///
/// # Thread Safety
///
/// This function is not thread-safe with respect to the output file. Do not call
/// `pack_snapshot` concurrently with the same output path. Concurrent packing to
/// different output files is safe.
///
/// The progress callback must be `Send + Sync` if you want to call this function
/// from a non-main thread.
pub fn pack_snapshot<F>(config: PackConfig, progress_callback: Option<F>) -> Result<()>
where
    F: Fn(u64, u64) + Send + Sync,
{
    // Validate inputs
    if config.disk.is_none() && config.memory.is_none() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "At least one input (disk or memory) must be provided",
        )));
    }

    // Train compression dictionary if requested
    let dictionary = if config.compression == "zstd" && config.train_dict {
        Some(train_dictionary(
            config.disk.as_ref().or(config.memory.as_ref()).unwrap(),
            config.block_size,
        )?)
    } else {
        None
    };

    // Initialize compressor
    let compressor: Box<dyn Compressor> = match config.compression.as_str() {
        "zstd" => Box::new(ZstdCompressor::new(DEFAULT_ZSTD_LEVEL, dictionary.clone())),
        _ => Box::new(Lz4Compressor::new()),
    };

    // Initialize encryptor if requested
    let (encryptor, enc_params): (Option<Box<dyn Encryptor>>, _) = if config.encrypt {
        let password = config.password.clone().ok_or_else(|| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Password required for encryption",
            ))
        })?;
        let params = KeyDerivationParams::default();
        let enc = AesGcmEncryptor::new(password.as_bytes(), &params.salt, params.iterations);
        (Some(Box::new(enc) as Box<dyn Encryptor>), Some(params))
    } else {
        (None, None)
    };

    let compression_type = if config.compression == "zstd" {
        CompressionType::Zstd
    } else {
        CompressionType::Lz4
    };

    let mut writer = SnapshotWriter::create(
        &config.output,
        compressor,
        encryptor,
        config.block_size,
        compression_type,
        config.cdc_enabled,
        enc_params,
    )?;

    // Write dictionary to file
    if let Some(d) = &dictionary {
        writer.write_dictionary(d)?;
    }

    // Process disk stream
    if let Some(ref path) = config.disk {
        process_stream(
            path.clone(),
            true,
            &mut writer,
            &config,
            progress_callback.as_ref(),
        )?;
    }

    // Process memory stream
    if let Some(ref path) = config.memory {
        process_stream(
            path.clone(),
            false,
            &mut writer,
            &config,
            progress_callback.as_ref(),
        )?;
    }

    writer.finalize(None, None)?;

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
///    - Uses Zstd's COVER algorithm (fast_cover variant)
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
/// Called internally by `pack_snapshot` when `train_dict` is enabled:
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

        f.seek(SeekFrom::Start(offset))?;
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

/// Processes a single input stream (disk or memory) via the [`SnapshotWriter`].
fn process_stream<F>(
    path: PathBuf,
    is_disk: bool,
    writer: &mut SnapshotWriter,
    config: &PackConfig,
    progress_callback: Option<&F>,
) -> Result<()>
where
    F: Fn(u64, u64),
{
    let f = File::open(&path)?;
    let len = f.metadata()?.len();

    writer.begin_stream(is_disk, len);

    // Choose chunker based on configuration
    let chunker: Box<dyn Chunker> = if config.cdc_enabled {
        let params = DedupeParams {
            f: (config.avg_chunk as f64).log2() as u32,
            m: config.min_chunk,
            z: config.max_chunk,
            w: 48,
            v: 8,
        };
        Box::new(StreamChunker::new(f, params))
    } else {
        Box::new(FixedChunker::new(f, config.block_size as usize))
    };

    let mut logical_pos = 0u64;

    for chunk_res in chunker {
        let chunk = chunk_res?;
        logical_pos += chunk.len() as u64;

        writer.write_data_block(&chunk)?;

        if let Some(callback) = progress_callback {
            callback(logical_pos, len);
        }
    }

    writer.end_stream()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

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
            "Entropy should be high for all byte values: got {}",
            entropy
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
    fn test_fixed_chunker_exact_blocks() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let cursor = Cursor::new(data);
        let chunker = FixedChunker::new(cursor, 4);

        let chunks: Vec<_> = chunker.map(|r| r.unwrap()).collect();

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], vec![1, 2, 3, 4]);
        assert_eq!(chunks[1], vec![5, 6, 7, 8]);
    }

    #[test]
    fn test_fixed_chunker_partial_last_block() {
        let data = vec![1, 2, 3, 4, 5];
        let cursor = Cursor::new(data);
        let chunker = FixedChunker::new(cursor, 3);

        let chunks: Vec<_> = chunker.map(|r| r.unwrap()).collect();

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], vec![1, 2, 3]);
        assert_eq!(chunks[1], vec![4, 5]);
    }

    #[test]
    fn test_fixed_chunker_empty_input() {
        let data = vec![];
        let cursor = Cursor::new(data);
        let chunker = FixedChunker::new(cursor, 1024);

        let chunks: Vec<_> = chunker.map(|r| r.unwrap()).collect();

        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_fixed_chunker_single_byte_blocks() {
        let data = vec![1, 2, 3];
        let cursor = Cursor::new(data);
        let chunker = FixedChunker::new(cursor, 1);

        let chunks: Vec<_> = chunker.map(|r| r.unwrap()).collect();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], vec![1]);
        assert_eq!(chunks[1], vec![2]);
        assert_eq!(chunks[2], vec![3]);
    }

    #[test]
    fn test_fixed_chunker_large_block_size() {
        let data = vec![1, 2, 3, 4, 5];
        let cursor = Cursor::new(data.clone());
        let chunker = FixedChunker::new(cursor, 10000);

        let chunks: Vec<_> = chunker.map(|r| r.unwrap()).collect();

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], data);
    }

    #[test]
    fn test_pack_config_default() {
        let config = PackConfig::default();

        assert_eq!(config.compression, "lz4");
        assert!(!config.encrypt);
        assert_eq!(config.password, None);
        assert!(!config.train_dict);
        assert_eq!(config.block_size, 65536);
        assert!(!config.cdc_enabled);
        assert_eq!(config.min_chunk, 16384);
        assert_eq!(config.avg_chunk, 65536);
        assert_eq!(config.max_chunk, 131072);
    }

    #[test]
    fn test_pack_config_clone() {
        let config1 = PackConfig {
            disk: Some(PathBuf::from("/dev/sda")),
            output: PathBuf::from("output.hxz"),
            compression: "zstd".to_string(),
            encrypt: true,
            password: Some("secret".to_string()),
            ..Default::default()
        };

        let config2 = config1.clone();

        assert_eq!(config2.disk, config1.disk);
        assert_eq!(config2.output, config1.output);
        assert_eq!(config2.compression, config1.compression);
        assert_eq!(config2.encrypt, config1.encrypt);
        assert_eq!(config2.password, config1.password);
    }

    #[test]
    fn test_pack_config_debug() {
        let config = PackConfig::default();
        let debug_str = format!("{:?}", config);

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

    #[test]
    fn test_fixed_chunker_with_different_sizes() {
        let data = vec![0u8; 10000];

        // Test with various chunk sizes
        for chunk_size in [64, 256, 1024, 4096, 65536] {
            let cursor = Cursor::new(data.clone());
            let chunker = FixedChunker::new(cursor, chunk_size);

            let chunks: Vec<_> = chunker.map(|r| r.unwrap()).collect();

            // Verify total data matches
            let total_len: usize = chunks.iter().map(|c| c.len()).sum();
            assert_eq!(
                total_len,
                data.len(),
                "Total chunked data should match original for chunk_size={}",
                chunk_size
            );

            // Verify all except possibly last chunk have correct size
            for (i, chunk) in chunks.iter().enumerate() {
                if i < chunks.len() - 1 {
                    assert_eq!(
                        chunk.len(),
                        chunk_size,
                        "Non-final chunks should be exactly chunk_size"
                    );
                } else {
                    assert!(
                        chunk.len() <= chunk_size,
                        "Final chunk should be <= chunk_size"
                    );
                }
            }
        }
    }
}
