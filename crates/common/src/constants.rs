//! Shared constants for the Hexz ecosystem.
//!
//! Tunable parameters governing performance, memory usage, and security.
//! Magic bytes, format version, and header size live in `hexz-core::format::magic`.

/// Default block size for archive compression (64 KiB).
///
/// Balances compression ratio, read amplification, and cache efficiency.
/// Smaller values (16-32 KiB) reduce read amplification for random I/O;
/// larger values (128-256 KiB) improve compression ratio and sequential throughput.
pub const DEFAULT_BLOCK_SIZE: u32 = 65536;

/// Default Zstd compression level.
///
/// Level 3 provides ~200 MB/s compression with good ratios (~2.5-3.5x on typical data).
/// Decompression speed is ~600+ MB/s regardless of level.
pub const DEFAULT_ZSTD_LEVEL: i32 = 3;

/// Salt size for PBKDF2 key derivation (128 bits).
///
/// Per NIST SP 800-132, 128 bits provides sufficient collision resistance.
pub const SALT_SIZE: usize = 16;

/// PBKDF2 iteration count for password-based key derivation.
///
/// 600,000 iterations per OWASP 2023 guidelines for PBKDF2-HMAC-SHA256.
/// Yields ~200-500ms key derivation on modern hardware.
pub const PBKDF2_ITERATIONS: u32 = 600_000;

/// Default block cache size (512 MiB).
///
/// Stores decompressed blocks in an LRU cache to avoid repeated decompression.
/// With 64 KiB blocks this holds ~8,192 blocks.
pub const DEFAULT_CACHE_SIZE: usize = 512 * 1024 * 1024;

/// Default number of blocks to prefetch ahead for sequential access patterns.
pub const DEFAULT_PREFETCH_COUNT: u32 = 4;

/// Default network timeout in seconds for HTTP/S3 backends.
pub const DEFAULT_NETWORK_TIMEOUT: u64 = 30;

/// AES-256 key length in bytes (256 bits).
pub const AES_KEY_LENGTH: usize = 32;

/// AES-GCM nonce length in bytes (96 bits).
///
/// Per NIST SP 800-38D, 96-bit nonces are recommended for AES-GCM.
/// Each block uses a deterministic nonce derived from the block index.
pub const AES_NONCE_LENGTH: usize = 12;

/// Shannon entropy threshold for dictionary training eligibility.
///
/// Blocks with entropy above this value (near-random data) are excluded from
/// dictionary training since they won't benefit from it.
pub const ENTROPY_THRESHOLD: f64 = 6.0;

/// Number of sample blocks for Zstd dictionary training.
pub const DICT_TRAINING_SAMPLE_COUNT: usize = 4000;

/// Maximum dictionary size for Zstd training (110 KiB).
///
/// Fits in L2 cache during decompression. Zstd's recommended max is 112 KiB.
pub const DICT_TRAINING_SIZE: usize = 110 * 1024;

/// Sentinel offset indicating a block is stored in the parent archive.
pub const BLOCK_OFFSET_PARENT: u64 = u64::MAX;

/// Overlay block granularity (4 KiB).
///
/// Matches standard filesystem block size for copy-on-write compatibility.
pub const OVERLAY_BLOCK_SIZE: u64 = 4096;

/// Size of a single overlay metadata entry in bytes (`u64` block index).
pub const META_ENTRY_SIZE: usize = 8;
