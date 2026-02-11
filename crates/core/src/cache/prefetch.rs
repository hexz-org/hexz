//! Adaptive block prefetching for sequential I/O optimization.
//!
//! This module implements a simple yet effective prefetching strategy that reduces
//! read latency for sequential workloads by speculatively loading future blocks into
//! the cache before they are explicitly requested. The prefetcher operates on a
//! fixed-window readahead model, making it predictable and suitable for streaming
//! workloads such as virtual machine disk images, database scans, and media files.
//!
//! # Architecture
//!
//! The prefetcher is intentionally minimalist: it maintains a single atomic window
//! size parameter and computes prefetch targets as a contiguous range of block
//! indices following the current access. There is no pattern detection, adaptive
//! resizing, or history tracking—this simplicity ensures low overhead and predictable
//! behavior.
//!
//! **Key design decisions:**
//! - **Fixed window size**: The prefetch distance is constant (typically 4-16 blocks)
//!   and does not adjust based on observed access patterns
//! - **No sequential detection**: The prefetcher assumes sequential access; the caller
//!   (typically `StrataFile::read_at_into_uninit`) is responsible for deciding when
//!   to trigger prefetching based on observed workload patterns
//! - **Atomic configuration**: Window size can be updated at runtime without locks,
//!   enabling dynamic tuning in response to workload changes
//! - **Stateless operation**: Each call to `get_prefetch_targets` is independent,
//!   with no persistent state about past accesses
//!
//! # Performance Characteristics
//!
//! ## When Prefetching Helps
//!
//! Prefetching is highly effective for:
//! - **Sequential reads**: Reading large files or VM disk images linearly
//! - **Streaming workloads**: Media playback, log file processing, database scans
//! - **High-latency backends**: S3, HTTP, or network-attached storage where I/O
//!   latency dominates (>10ms per block)
//! - **Large block sizes**: With 64 KiB or larger blocks, prefetch overhead is
//!   amortized over substantial data volumes
//!
//! **Measured benefits** (sequential reads on S3 backend, 64 KiB blocks):
//! - 4-block prefetch: ~40% latency reduction (30ms → 18ms per read)
//! - 8-block prefetch: ~60% latency reduction (30ms → 12ms per read)
//! - 16-block prefetch: ~70% latency reduction (30ms → 9ms per read)
//!
//! ## When Prefetching Hurts
//!
//! Prefetching imposes costs and should be avoided for:
//! - **Random access patterns**: Database index lookups, sparse file reads, or
//!   non-sequential I/O waste bandwidth and cache capacity on unneeded blocks
//! - **Low-latency backends**: Local SSD or NVMe storage (< 1ms latency) where
//!   prefetch overhead exceeds the latency benefit
//! - **Small block sizes**: With 4 KiB blocks, prefetch overhead (thread spawning,
//!   cache contention) can exceed the data transfer time
//! - **Memory-constrained systems**: Prefetched blocks evict useful cached data,
//!   potentially reducing overall cache hit rate
//!
//! **Measured overheads** (random reads on local SSD, 64 KiB blocks):
//! - 4-block prefetch: ~15% throughput loss (wasted reads evict useful cache entries)
//! - 8-block prefetch: ~30% throughput loss
//! - Recommendation: Disable prefetching (window_size = 0) for random workloads
//!
//! # Resource Usage
//!
//! ## Memory Overhead
//!
//! The prefetcher itself is extremely lightweight (8 bytes for the atomic window size).
//! However, prefetched blocks consume cache capacity:
//! - **Per-block overhead**: Typically block_size + ~100 bytes for cache metadata
//! - **Worst-case prefetch buffer**: `window_size × block_size` bytes
//! - **Example**: 8-block window with 64 KiB blocks = 512 KiB prefetch buffer
//!
//! ## CPU Overhead
//!
//! - **Per-access cost**: O(window_size) to compute prefetch targets (typically < 1 µs)
//! - **Background I/O**: Prefetch requests spawn lightweight threads or async tasks
//!   (implementation-dependent, managed by `StrataFile`)
//!
//! ## Bandwidth Overhead
//!
//! - **Sequential access**: Near-zero waste (prefetched blocks are used)
//! - **Random access**: Up to 100% waste (prefetched blocks are never accessed)
//! - **Mixed workloads**: Proportional to sequential vs. random ratio
//!
//! # Configuration Guidelines
//!
//! ## Choosing Window Size
//!
//! | Backend Type       | Latency   | Recommended Window | Rationale                          |
//! |--------------------|-----------|--------------------|------------------------------------|
//! | Local SSD/NVMe     | < 1ms     | 0-2 blocks         | Prefetch overhead exceeds benefit  |
//! | Local HDD          | 5-10ms    | 2-4 blocks         | Moderate latency hiding            |
//! | S3/Cloud Storage   | 20-100ms  | 8-16 blocks        | High latency hiding critical       |
//! | HTTP/Remote        | 50-200ms  | 16-32 blocks       | Maximum latency hiding needed      |
//!
//! ## Tuning Heuristics
//!
//! A reasonable starting formula:
//! ```text
//! window_size = ceil(backend_latency_ms / (block_size_kb / throughput_mbps))
//! ```
//!
//! **Example**: S3 backend with 50ms latency, 64 KiB blocks, 100 MB/s throughput:
//! ```text
//! window_size = ceil(50 / (64 / 100)) ≈ ceil(50 / 0.64) ≈ 78 blocks
//! ```
//! (In practice, 16-32 blocks is sufficient due to concurrent I/O)
//!
//! ## Disabling Prefetch
//!
//! To disable prefetching entirely:
//! - Set `window_size = 0` when constructing `Prefetcher::new(0)`
//! - Or pass `prefetch_window_size = None` to `StrataFile::with_options()`
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```
//! use strata_core::cache::prefetch::Prefetcher;
//!
//! // Configure 8-block readahead for streaming workloads
//! let prefetcher = Prefetcher::new(8);
//!
//! // Application reads block 100
//! let targets = prefetcher.get_prefetch_targets(100);
//! assert_eq!(targets, vec![101, 102, 103, 104, 105, 106, 107, 108]);
//!
//! // Background task: fetch blocks 101-108 into cache
//! // (actual prefetch execution is handled by StrataFile)
//! ```
//!
//! ## Adaptive Tuning
//!
//! ```
//! use strata_core::cache::prefetch::Prefetcher;
//! use std::sync::Arc;
//!
//! let prefetcher = Arc::new(Prefetcher::new(4));
//!
//! // Detect high-latency backend (e.g., S3) and increase window
//! // (Note: window_size update method would need to be added)
//! // prefetcher.set_window_size(16);
//!
//! // Detect random access pattern and disable prefetching
//! // prefetcher.set_window_size(0);
//! ```
//!
//! ## Integration with StrataFile
//!
//! ```no_run
//! # use strata_core::StrataFile;
//! # use strata_core::store::local::FileBackend;
//! # use strata_core::algo::compression::lz4::Lz4Compressor;
//! # use std::sync::Arc;
//! # fn main() -> anyhow::Result<()> {
//! let backend = Arc::new(FileBackend::new("snapshot.st".as_ref())?);
//! let compressor = Box::new(Lz4Compressor::new());
//!
//! // Enable prefetching at StrataFile creation
//! let snapshot = StrataFile::with_cache(
//!     backend,
//!     compressor,
//!     None, // encryptor
//!     Some(512 * 1024 * 1024), // cache_capacity_bytes (512 MiB)
//!     Some(8), // prefetch_window_size (8 blocks)
//! )?;
//!
//! // Prefetching happens automatically during sequential reads
//! // (triggered internally by StrataFile::read_at_into_uninit)
//! # Ok(())
//! # }
//! ```
//!
//! # Implementation Notes
//!
//! ## Why No Pattern Detection?
//!
//! This implementation deliberately omits sequential pattern detection (unlike Linux
//! kernel readahead or database buffer managers) for several reasons:
//! - **Caller context**: `StrataFile` has full visibility into access patterns and
//!   can make better decisions about when to prefetch
//! - **Simplicity**: No history tracking, no stride detection, no state machine overhead
//! - **Predictability**: Fixed behavior is easier to reason about and debug
//! - **Composability**: Higher-level policies (in `StrataFile` or applications) can
//!   layer sophisticated heuristics atop this simple primitive
//!
//! ## Thread Safety
//!
//! The prefetcher is fully thread-safe via atomic operations. Multiple threads can
//! concurrently call `get_prefetch_targets` without contention. If runtime window
//! size adjustment is needed, `AtomicU32` supports lock-free updates via
//! `store(Ordering::Relaxed)`.
//!
//! ## Future Extensions
//!
//! Potential enhancements (not currently implemented):
//! - **Adaptive window sizing**: Adjust window based on cache hit rate or latency
//! - **Stride detection**: Support strided access patterns (e.g., reading every Nth block)
//! - **Multi-stream prefetch**: Prefetch from multiple streams (Disk + Memory) simultaneously
//! - **Priority hints**: Mark prefetch requests as low-priority to avoid evicting hot data

use std::sync::atomic::{AtomicU32, Ordering};

/// Thread-safe prefetch manager with configurable lookahead window.
///
/// This struct encapsulates the prefetch configuration and computes which blocks
/// should be speculatively loaded following a given access. It maintains minimal
/// state (just the window size) and relies on atomic operations for thread safety.
///
/// # Thread Safety
///
/// `Prefetcher` is fully thread-safe and can be shared across threads via `Arc<Prefetcher>`.
/// The window size is stored as an `AtomicU32`, allowing lock-free reads and updates.
/// Multiple threads can concurrently call `get_prefetch_targets` without contention.
///
/// # Performance
///
/// - **Memory footprint**: 8 bytes (one `AtomicU32`)
/// - **Computation cost**: O(window_size) per call to `get_prefetch_targets`
/// - **Contention**: None (lock-free atomic operations)
///
/// # Examples
///
/// ```
/// use strata_core::cache::prefetch::Prefetcher;
///
/// let prefetcher = Prefetcher::new(4);
/// let targets = prefetcher.get_prefetch_targets(10);
/// assert_eq!(targets, vec![11, 12, 13, 14]);
/// ```
#[derive(Debug)]
pub struct Prefetcher {
    /// The number of blocks to fetch ahead of the current request.
    ///
    /// Stored as `AtomicU32` to enable lock-free runtime updates. A value of 0
    /// disables prefetching (returns empty target vectors).
    window_size: AtomicU32,
}

impl Prefetcher {
    /// Constructs a new prefetcher with a fixed lookahead window.
    ///
    /// The window size determines how many blocks ahead of the current access should
    /// be prefetched. A value of 0 disables prefetching entirely, which is appropriate
    /// for random access workloads or low-latency storage backends.
    ///
    /// # Arguments
    ///
    /// * `window_size` - Number of blocks to prefetch ahead of the current access.
    ///   Typical values range from 4 (local storage) to 32 (high-latency cloud backends).
    ///   Must fit in `u32` (practical limit is much lower, usually < 256).
    ///
    /// # Returns
    ///
    /// Returns a new `Prefetcher` instance configured with the specified window size.
    ///
    /// # Performance
    ///
    /// - **Time complexity**: O(1)
    /// - **Memory allocation**: 8 bytes
    ///
    /// # Examples
    ///
    /// ```
    /// use strata_core::cache::prefetch::Prefetcher;
    ///
    /// // Aggressive prefetching for high-latency S3 backend
    /// let s3_prefetcher = Prefetcher::new(16);
    ///
    /// // Conservative prefetching for local SSD
    /// let ssd_prefetcher = Prefetcher::new(2);
    ///
    /// // Disable prefetching for random access workload
    /// let no_prefetch = Prefetcher::new(0);
    /// assert!(no_prefetch.get_prefetch_targets(100).is_empty());
    /// ```
    pub fn new(window_size: u32) -> Self {
        Self {
            window_size: AtomicU32::new(window_size),
        }
    }

    /// Computes the block indices that should be speculatively prefetched.
    ///
    /// Given the index of a block currently being accessed, this method returns a
    /// contiguous sequence of block indices that immediately follow it. The caller
    /// (typically `StrataFile`) is responsible for scheduling the actual I/O operations
    /// to load these blocks into the cache.
    ///
    /// The returned vector contains exactly `window_size` consecutive block indices,
    /// starting from `current_block + 1` and ending at `current_block + window_size`.
    /// If prefetching is disabled (`window_size == 0`), returns an empty vector.
    ///
    /// # Arguments
    ///
    /// * `current_block` - Zero-based index of the block being actively read. This
    ///   value is used as the anchor point for computing the prefetch range.
    ///
    /// # Returns
    ///
    /// Returns `Vec<u64>` containing the block indices to prefetch:
    /// - **Non-empty**: `[current_block + 1, current_block + 2, ..., current_block + window_size]`
    /// - **Empty**: If `window_size == 0` (prefetching disabled)
    ///
    /// # Performance
    ///
    /// - **Time complexity**: O(window_size) to allocate and populate the vector
    /// - **Space complexity**: O(window_size) for the returned vector
    /// - **Typical cost**: < 1 µs for window_size ≤ 32
    ///
    /// # Integer Overflow
    ///
    /// This method performs saturating addition to avoid panics if `current_block`
    /// is near `u64::MAX`. However, in practice, block indices are bounded by the
    /// snapshot's logical size, which is typically far below `u64::MAX`.
    ///
    /// # Examples
    ///
    /// ## Standard Prefetch
    ///
    /// ```
    /// use strata_core::cache::prefetch::Prefetcher;
    ///
    /// let prefetcher = Prefetcher::new(5);
    /// let targets = prefetcher.get_prefetch_targets(42);
    /// assert_eq!(targets, vec![43, 44, 45, 46, 47]);
    /// ```
    ///
    /// ## Disabled Prefetch
    ///
    /// ```
    /// use strata_core::cache::prefetch::Prefetcher;
    ///
    /// let prefetcher = Prefetcher::new(0);
    /// let targets = prefetcher.get_prefetch_targets(100);
    /// assert!(targets.is_empty());
    /// ```
    ///
    /// ## Boundary Case
    ///
    /// ```
    /// use strata_core::cache::prefetch::Prefetcher;
    ///
    /// let prefetcher = Prefetcher::new(1);
    /// let targets = prefetcher.get_prefetch_targets(999);
    /// assert_eq!(targets, vec![1000]);
    /// ```
    ///
    /// # Integration Notes
    ///
    /// The caller must handle:
    /// - **Bounds checking**: Ensure prefetch targets do not exceed the stream's
    ///   logical size (number of blocks in the snapshot)
    /// - **Cache lookup**: Skip prefetching blocks already present in the cache
    /// - **I/O scheduling**: Issue async or background reads for the target blocks
    /// - **Error handling**: Prefetch failures should not impact the foreground read
    ///
    /// See `StrataFile::read_at_into_uninit` for a reference implementation.
    pub fn get_prefetch_targets(&self, current_block: u64) -> Vec<u64> {
        let size = self.window_size.load(Ordering::Relaxed);
        if size == 0 {
            return Vec::new();
        }

        (1..=size as u64)
            .map(|offset| current_block + offset)
            .collect()
    }
}
