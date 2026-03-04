//! Global Configuration Structure Definitions.
//!
//! This module defines the configuration parameters that control the behavior
//! of the filesystem, including cache sizing, prefetching policies, and
//! network timeouts. It allows for fine-tuning the performance characteristics
//! based on the deployment environment and available system resources.

use crate::constants::{DEFAULT_CACHE_SIZE, DEFAULT_NETWORK_TIMEOUT, DEFAULT_PREFETCH_COUNT};

/// Pre-defined optimization profiles for the `hexz build` command.
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
    #[must_use]
    pub const fn block_size(&self) -> u32 {
        match self {
            Self::Generic => 65_536, // 64 KiB
            Self::Eda => 16_384,     // 16 KiB
            Self::Embedded => 4_096, // 4 KiB
            Self::Ml => 1_048_576,    // 1 MiB
        }
    }

    /// Returns the recommended compression algorithm for this profile.
    /// Note: This returns a string compatible with the CLI argument parser.
    #[must_use]
    pub const fn compression_algo(&self) -> &'static str {
        match self {
            Self::Generic => "lz4",
            Self::Eda => "zstd",
            Self::Embedded => "zstd",
            Self::Ml => "lz4",
        }
    }

    /// Whether this profile recommends dictionary training.
    #[must_use]
    pub const fn recommended_dict_training(&self) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_profile_generic() {
        let profile = BuildProfile::Generic;
        assert_eq!(profile.block_size(), 65_536);
        assert_eq!(profile.compression_algo(), "lz4");
        assert!(!profile.recommended_dict_training());
    }

    #[test]
    fn test_build_profile_eda() {
        let profile = BuildProfile::Eda;
        assert_eq!(profile.block_size(), 16_384);
        assert_eq!(profile.compression_algo(), "zstd");
        assert!(profile.recommended_dict_training());
    }

    #[test]
    fn test_build_profile_embedded() {
        let profile = BuildProfile::Embedded;
        assert_eq!(profile.block_size(), 4_096);
        assert_eq!(profile.compression_algo(), "zstd");
        assert!(profile.recommended_dict_training());
    }

    #[test]
    fn test_build_profile_ml() {
        let profile = BuildProfile::Ml;
        assert_eq!(profile.block_size(), 1_048_576);
        assert_eq!(profile.compression_algo(), "lz4");
        assert!(!profile.recommended_dict_training());
    }

    #[test]
    fn test_build_profile_equality() {
        assert_eq!(BuildProfile::Generic, BuildProfile::Generic);
        assert_eq!(BuildProfile::Eda, BuildProfile::Eda);
        assert_ne!(BuildProfile::Generic, BuildProfile::Eda);
        assert_ne!(BuildProfile::Embedded, BuildProfile::Ml);
    }

    #[test]
    fn test_build_profile_copy() {
        let profile1 = BuildProfile::Generic;
        let profile2 = profile1; // Copy

        assert_eq!(profile1, profile2);
        assert_eq!(profile1.block_size(), profile2.block_size());
    }

    #[test]
    fn test_build_profile_clone() {
        let profile1 = BuildProfile::Eda;
        let profile2 = profile1;

        assert_eq!(profile1, profile2);
    }

    #[test]
    fn test_build_profile_debug() {
        let profile = BuildProfile::Ml;
        let debug_str = format!("{profile:?}");

        assert!(debug_str.contains("Ml"));
    }

    #[test]
    fn test_config_default() {
        let config = Config::default();

        assert_eq!(config.cache_size_bytes, DEFAULT_CACHE_SIZE);
        assert_eq!(config.prefetch_count, DEFAULT_PREFETCH_COUNT);
        assert_eq!(config.network_timeout_secs, DEFAULT_NETWORK_TIMEOUT);
    }

    #[test]
    fn test_config_clone() {
        let config1 = Config {
            cache_size_bytes: 1024 * 1024 * 1024,
            prefetch_count: 8,
            network_timeout_secs: 60,
        };

        let config2 = config1.clone();

        assert_eq!(config2.cache_size_bytes, 1024 * 1024 * 1024);
        assert_eq!(config2.prefetch_count, 8);
        assert_eq!(config2.network_timeout_secs, 60);
    }

    #[test]
    fn test_config_debug() {
        let config = Config::default();
        let debug_str = format!("{config:?}");

        assert!(debug_str.contains("Config"));
        assert!(debug_str.contains("cache_size_bytes"));
        assert!(debug_str.contains("prefetch_count"));
        assert!(debug_str.contains("network_timeout_secs"));
    }

    #[test]
    fn test_config_custom_values() {
        let config = Config {
            cache_size_bytes: 2048,
            prefetch_count: 16,
            network_timeout_secs: 120,
        };

        assert_eq!(config.cache_size_bytes, 2048);
        assert_eq!(config.prefetch_count, 16);
        assert_eq!(config.network_timeout_secs, 120);
    }

    #[test]
    fn test_build_profile_all_variants() {
        // Ensure all variants can be constructed
        let _ = BuildProfile::Generic;
        let _ = BuildProfile::Eda;
        let _ = BuildProfile::Embedded;
        let _ = BuildProfile::Ml;
    }

    #[test]
    fn test_build_profile_block_sizes_ordered() {
        // Verify block sizes make sense
        assert!(BuildProfile::Embedded.block_size() < BuildProfile::Eda.block_size());
        assert!(BuildProfile::Eda.block_size() < BuildProfile::Generic.block_size());
        assert!(BuildProfile::Generic.block_size() < BuildProfile::Ml.block_size());
    }

    #[test]
    fn test_compression_algo_returns_valid_strings() {
        assert_eq!(BuildProfile::Generic.compression_algo(), "lz4");
        assert_eq!(BuildProfile::Eda.compression_algo(), "zstd");
        assert_eq!(BuildProfile::Embedded.compression_algo(), "zstd");
        assert_eq!(BuildProfile::Ml.compression_algo(), "lz4");
    }

    #[test]
    fn test_dict_training_recommendations() {
        // Generic and ML don't need dictionary training
        assert!(!BuildProfile::Generic.recommended_dict_training());
        assert!(!BuildProfile::Ml.recommended_dict_training());

        // EDA and Embedded benefit from dictionary training
        assert!(BuildProfile::Eda.recommended_dict_training());
        assert!(BuildProfile::Embedded.recommended_dict_training());
    }
}
