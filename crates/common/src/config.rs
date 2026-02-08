//! Global Configuration Structure Definitions.
//!
//! This module defines the configuration parameters that control the behavior
//! of the filesystem, including cache sizing, prefetching policies, and
//! network timeouts. It allows for fine-tuning the performance characteristics
//! based on the deployment environment and available system resources.

use crate::constants::{DEFAULT_CACHE_SIZE, DEFAULT_NETWORK_TIMEOUT, DEFAULT_PREFETCH_COUNT};

/// Pre-defined optimization profiles for the `strata build` command.
///
/// **Architectural intent:** Simplifies the configuration surface for common
/// use cases by grouping block size, compression, and alignment settings into
/// named presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildProfile {
    /// Balanced defaults for general-purpose use (64 KiB blocks, LZ4/Zstd).
    Generic,
    /// EDA/Text focus: smaller blocks (16 KiB) and dictionary compression.
    Eda,
    /// Embedded systems: high compression (Zstd), small blocks (4 KiB).
    Embedded,
    /// Machine Learning: columnar alignment, large blocks (e.g., 1 MiB) or matched to tensor sizes.
    Ml,
}

impl BuildProfile {
    /// Returns the recommended block size for this profile.
    pub fn block_size(&self) -> u32 {
        match self {
            Self::Generic => 65536, // 64 KiB
            Self::Eda => 16384,     // 16 KiB
            Self::Embedded => 4096, // 4 KiB
            Self::Ml => 1048576,    // 1 MiB
        }
    }

    /// Returns the recommended compression algorithm for this profile.
    /// Note: This returns a string compatible with the CLI argument parser.
    pub fn compression_algo(&self) -> &'static str {
        match self {
            Self::Generic => "lz4",
            Self::Eda => "zstd",
            Self::Embedded => "zstd",
            Self::Ml => "lz4",
        }
    }

    /// Whether this profile recommends dictionary training.
    pub fn recommended_dict_training(&self) -> bool {
        match self {
            Self::Generic => false,
            Self::Eda => true,
            Self::Embedded => true,
            Self::Ml => false,
        }
    }
}

/// Aggregated configuration for the filesystem runtime.
///
/// This struct holds all tunable parameters for the system. It is typically
/// constructed from command-line arguments or a configuration file and passed
/// down to the core components during initialization. The configuration
/// affects memory usage, I/O behavior, and network operation timeouts.
#[derive(Debug, Clone)]
pub struct Config {
    /// The maximum size of the in-memory block cache in bytes.
    ///
    /// This parameter controls the memory footprint of the application.
    /// A larger cache improves read performance for repeated access but
    /// consumes more system RAM. The cache uses an LRU eviction policy
    /// when this limit is reached.
    pub cache_size_bytes: usize,

    /// The number of blocks to prefetch sequentially during read operations.
    ///
    /// This setting optimizes read throughput for sequential access patterns
    /// by fetching ahead of the request cursor. A value of 0 disables prefetching,
    /// which may be desirable for random access workloads where prefetching
    /// would waste bandwidth.
    pub prefetch_count: u32,

    /// The timeout duration in seconds for network operations.
    ///
    /// This applies to remote storage backends like S3 or HTTP. It ensures
    /// that operations do not hang indefinitely in case of network partitions
    /// or unresponsive servers. Operations that exceed this timeout will
    /// return an I/O error.
    pub network_timeout_secs: u64,
}

impl Default for Config {
    /// Provides sensible default values for the configuration.
    ///
    /// These defaults are chosen to provide a balance between performance
    /// and resource usage for a typical desktop environment: 512MB cache,
    /// 4-block prefetch, and 30-second network timeout. These values can be
    /// overridden based on available system resources and workload characteristics.
    ///
    /// # Returns
    ///
    /// Returns a new `Config` instance with default values.
    fn default() -> Self {
        Self {
            cache_size_bytes: DEFAULT_CACHE_SIZE,
            prefetch_count: DEFAULT_PREFETCH_COUNT,
            network_timeout_secs: DEFAULT_NETWORK_TIMEOUT,
        }
    }
}
