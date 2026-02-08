//! Shared constants and magic numbers for the Strata ecosystem.
//!
//! **Note:** Magic bytes, format version, and header size have been moved
//! to `strata-core::format::magic` as they are format-specific constants.

/// Default block size for snapshots (64 KiB).
pub const DEFAULT_BLOCK_SIZE: u32 = 65536;

/// Default compression level for Zstd.
pub const DEFAULT_ZSTD_LEVEL: i32 = 3;

/// Size of the salt used for key derivation.
pub const SALT_SIZE: usize = 16;

/// Number of iterations for PBKDF2 key derivation.
pub const PBKDF2_ITERATIONS: u32 = 600_000;

/// Default cache size (512 MiB).
pub const DEFAULT_CACHE_SIZE: usize = 512 * 1024 * 1024;

/// Default prefetch window size.
pub const DEFAULT_PREFETCH_COUNT: u32 = 4;

/// Default network timeout in seconds.
pub const DEFAULT_NETWORK_TIMEOUT: u64 = 30;

/// AES-256 Key length in bytes.
pub const AES_KEY_LENGTH: usize = 32;

/// AES-GCM Nonce length in bytes.
pub const AES_NONCE_LENGTH: usize = 12;

/// Entropy threshold for dictionary training filter.
pub const ENTROPY_THRESHOLD: f64 = 6.0;

/// Target sample count for dictionary training.
pub const DICT_TRAINING_SAMPLE_COUNT: usize = 4000;

/// Max size for dictionary training data.
pub const DICT_TRAINING_SIZE: usize = 110 * 1024;

/// Sentinel offset value indicating the block is stored in the parent snapshot.
pub const BLOCK_OFFSET_PARENT: u64 = u64::MAX;
