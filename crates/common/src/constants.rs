//! Shared constants and magic numbers for the Strata ecosystem.
//!
//! **Note:** Magic bytes, format version, and header size have been moved
//! to `strata-core::format::magic` as they are format-specific constants.
//!
//! # Performance Tuning
//!
//! Many constants in this module directly affect performance and security tradeoffs:
//!
//! - **Block size**: Larger blocks reduce metadata overhead but increase minimum I/O granularity
//! - **Compression level**: Higher levels improve ratios but reduce throughput
//! - **Cache size**: Larger caches improve hit rates but consume more memory
//! - **PBKDF2 iterations**: More iterations increase security but slow key derivation
//!
//! See individual constant documentation for specific tuning guidance.

/// Default block size for snapshots (64 KiB).
///
/// # Performance Implications
///
/// This value balances several competing factors:
/// - **Compression efficiency**: 64 KiB blocks provide good compression ratios without
///   excessive dictionary overhead
/// - **I/O granularity**: Minimum read size is one block; smaller blocks reduce wasted I/O
///   but increase metadata size
/// - **Memory usage**: Decompression buffers are allocated per block
/// - **Deduplication**: Larger blocks reduce dedup opportunities; smaller blocks increase
///   metadata overhead
///
/// **Typical performance**: 64 KiB achieves 70-80% of maximum compression ratio while
/// maintaining low read amplification for random access patterns.
///
/// **When to adjust**:
/// - Increase to 128-256 KiB for sequential workloads or highly compressible data
/// - Decrease to 16-32 KiB for random-access workloads or low memory environments
pub const DEFAULT_BLOCK_SIZE: u32 = 65536;

/// Default compression level for Zstd (level 3).
///
/// # Performance Implications
///
/// Zstd level 3 is chosen to balance compression ratio and throughput:
/// - **Compression speed**: ~200 MB/s (single-threaded on modern CPUs)
/// - **Decompression speed**: ~600 MB/s (single-threaded)
/// - **Compression ratio**: Typically 2.5-4x for VM disk images
///
/// **When to adjust**:
/// - Level 1: Faster compression (~400 MB/s) with slightly lower ratio (2-3x)
/// - Level 5-7: Better ratio (3-5x) but slower compression (50-100 MB/s)
/// - Level 10+: Maximum ratio but very slow (not recommended for real-time use)
///
/// Note: Dictionary training can improve ratios by 10-30% at any compression level.
pub const DEFAULT_ZSTD_LEVEL: i32 = 3;

/// Size of the salt used for key derivation (16 bytes / 128 bits).
///
/// # Security Rationale
///
/// 128-bit salts provide sufficient entropy to prevent rainbow table attacks
/// and ensure unique derived keys even when the same password is used multiple
/// times. This follows NIST SP 800-132 recommendations for password-based key
/// derivation.
pub const SALT_SIZE: usize = 16;

/// Number of iterations for PBKDF2 key derivation (600,000).
///
/// # Security Implications
///
/// This value is chosen based on OWASP recommendations (2023) for PBKDF2-HMAC-SHA256:
/// - **Minimum recommended**: 600,000 iterations
/// - **Time cost**: ~500 ms on a 2020-era CPU (acceptable for interactive use)
/// - **Attack resistance**: Makes brute-force attacks computationally expensive
///
/// **Performance impact**: Key derivation occurs once per snapshot open/create operation.
/// The 500ms delay is acceptable for interactive use but may be problematic for
/// automated pipelines opening many encrypted snapshots.
///
/// **When to adjust**:
/// - Increase to 1,000,000+ for high-security environments where slower key derivation
///   is acceptable
/// - Do not decrease below 600,000 unless required for compatibility with older snapshots
pub const PBKDF2_ITERATIONS: u32 = 600_000;

/// Default cache size (512 MiB).
///
/// # Performance Implications
///
/// The block cache stores recently decompressed blocks to avoid repeated decompression:
/// - **Hit rate**: 512 MiB typically achieves 80-95% cache hit rates for typical VM workloads
/// - **Memory overhead**: Fixed allocation on snapshot open
/// - **Compression savings**: Each cache hit saves one decompression operation (~1-2 µs)
///
/// **When to adjust**:
/// - Increase to 1-2 GiB for memory-rich environments or large working sets
/// - Decrease to 128-256 MiB for memory-constrained environments
/// - Set to 0 to disable caching (not recommended; severely impacts performance)
///
/// **Measurement**: Monitor cache hit rates using snapshot metadata/profiling to
/// optimize this value for your workload.
pub const DEFAULT_CACHE_SIZE: usize = 512 * 1024 * 1024;

/// Default prefetch window size (4 blocks).
///
/// # Performance Implications
///
/// Prefetching reads ahead by N blocks during sequential access to hide I/O latency:
/// - **Latency hiding**: With 64 KiB blocks, 4-block prefetch covers ~1ms of processing time
/// - **Memory overhead**: Prefetch buffer holds up to 256 KiB (4 × 64 KiB) of compressed data
/// - **Sequential detection**: Only activates when sequential access pattern is detected
///
/// **When to adjust**:
/// - Increase to 8-16 for high-latency backends (S3, HTTP) or fast sequential reads
/// - Decrease to 2 for low-latency backends (local SSD) or random access patterns
/// - Set to 0 to disable prefetching
pub const DEFAULT_PREFETCH_COUNT: u32 = 4;

/// Default network timeout in seconds (30 seconds).
///
/// # Performance Implications
///
/// This timeout applies to individual HTTP/S3 requests for remote storage backends:
/// - **Typical request time**: 50-500 ms for S3 block reads (depending on region/size)
/// - **Timeout headroom**: 30 seconds allows for network congestion and retries
/// - **Failure detection**: Shorter timeouts fail faster but may cause false positives
///
/// **When to adjust**:
/// - Increase to 60+ seconds for high-latency or unreliable networks
/// - Decrease to 10-15 seconds for low-latency networks where fast failure is preferred
///
/// Note: This is a per-request timeout, not a total operation timeout.
pub const DEFAULT_NETWORK_TIMEOUT: u64 = 30;

/// AES-256 Key length in bytes.
pub const AES_KEY_LENGTH: usize = 32;

/// AES-GCM Nonce length in bytes.
pub const AES_NONCE_LENGTH: usize = 12;

/// Entropy threshold for dictionary training filter (6.0 bits per byte).
///
/// # Performance Implications
///
/// Blocks with entropy below this threshold are excluded from dictionary training
/// samples because they are likely highly compressible already or contain repeating
/// patterns that don't benefit from dictionary compression.
///
/// - **Value range**: 0-8 bits per byte (Shannon entropy)
/// - **Typical values**: Random data ~7.9, text ~4-5, zeros ~0
/// - **Threshold rationale**: 6.0 excludes very low-entropy blocks while including
///   structured data that benefits from dictionary training
///
/// **When to adjust**:
/// - Increase to 7.0 to focus on more random/complex data
/// - Decrease to 5.0 to include more structured data in training
pub const ENTROPY_THRESHOLD: f64 = 6.0;

/// Target sample count for dictionary training (4000 samples).
///
/// # Performance Implications
///
/// Zstd dictionary training requires a corpus of sample data:
/// - **Sample size**: Each sample is typically one block (64 KiB by default)
/// - **Total corpus**: 4000 × 64 KiB = ~256 MiB of training data
/// - **Training time**: ~2-5 seconds to analyze and build dictionary
/// - **Ratio improvement**: Well-trained dictionaries can improve compression by 10-30%
///
/// **When to adjust**:
/// - Increase to 8000+ for very large, diverse datasets
/// - Decrease to 2000 for faster training or smaller snapshots
///
/// Note: More samples improve dictionary quality but increase training time linearly.
pub const DICT_TRAINING_SAMPLE_COUNT: usize = 4000;

/// Max size for dictionary training data (110 KiB).
///
/// # Performance Implications
///
/// The final trained dictionary is capped at this size:
/// - **Size rationale**: 110 KiB is large enough to capture common patterns but small
///   enough to fit in L2 cache during decompression
/// - **Memory overhead**: Dictionary is loaded once per snapshot and shared across threads
/// - **Compression benefit**: Larger dictionaries improve ratio but slow decompression
///   due to cache pressure
///
/// Zstd's default dictionary size limit is 112 KiB; we use 110 KiB to leave headroom
/// for metadata.
pub const DICT_TRAINING_SIZE: usize = 110 * 1024;

/// Sentinel offset value indicating the block is stored in the parent snapshot.
pub const BLOCK_OFFSET_PARENT: u64 = u64::MAX;
