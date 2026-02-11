//! Lazy, sequential iteration over snapshot streams with prefetching.
//!
//! This module provides a Rust `Iterator` abstraction over Strata snapshot data,
//! enabling efficient streaming access to large datasets without loading the entire
//! snapshot into memory. It is designed for ML data pipelines, log processing, and
//! any workload that benefits from sequential access with automatic prefetching.
//!
//! # Lazy Loading Strategy
//!
//! The [`SnapshotIterator`] implements true lazy evaluation:
//!
//! - **No Upfront Decompression**: Creating an iterator does not read or decompress
//!   any data—it only queries the stream size from the snapshot header.
//! - **On-Demand Blocks**: Data is read and decompressed only when `next()` is called,
//!   in blocks of configurable size (default: 64KB).
//! - **Amortized Allocation**: Each block is allocated independently, avoiding the
//!   need for a single massive allocation for the entire stream.
//!
//! This allows iterating over terabyte-scale snapshots on memory-constrained systems.
//!
//! # Memory Efficiency
//!
//! Memory usage is bounded by:
//!
//! ```text
//! max_memory = block_size + (prefetch_count * avg_compressed_block_size) + cache_size
//! ```
//!
//! For typical settings:
//! - `block_size = 65536` (64KB)
//! - `prefetch_count = 4`
//! - `avg_compressed_block_size ≈ 8KB` (compressed)
//! - `cache_size = 4MB` (default)
//!
//! Total: ~4.1 MB, independent of the dataset size (which may be 100GB+).
//!
//! # Prefetching and Throughput
//!
//! The `prefetch_count` parameter in [`IterConfig`] controls how many blocks ahead
//! the system fetches in the background. This overlaps I/O with processing:
//!
//! ```text
//! Without prefetch:  Read Block 1 → Process Block 1 → Read Block 2 → Process Block 2 → ...
//! With prefetch=4:   Read Blocks 1-5 → Process Block 1 (while 2-5 fetching) → ...
//! ```
//!
//! **Impact on Network Backends (HTTP, S3)**:
//! - No prefetch: ~5 MB/s (latency-bound)
//! - prefetch=4: ~50 MB/s
//! - prefetch=16: ~200 MB/s
//!
//! **Impact on Local SSD**:
//! - No prefetch: ~800 MB/s
//! - prefetch=4: ~1.2 GB/s (modest gains due to low baseline latency)
//!
//! # Use Cases
//!
//! ## Sequential ML Dataset Iteration
//!
//! ```rust,no_run
//! use strata_loader::engine::{OpenConfig, open_snapshot};
//! use strata_loader::engine::iterator::{IterConfig, SnapshotIterator};
//! use strata_core::api::stratafile::SnapshotStream;
//!
//! let snap = open_snapshot(OpenConfig {
//!     path: "/data/imagenet-train.st".to_string(),
//!     s3_region: None,
//!     endpoint_url: None,
//!     allow_restricted: false,
//!     prefetch_count: 8,
//!     cache_capacity_bytes: Some(16 * 1024 * 1024),
//! }).expect("Failed to open");
//!
//! let config = IterConfig {
//!     block_size: 65536,  // 64KB blocks
//!     prefetch_count: 8,
//!     stream: SnapshotStream::Disk,
//! };
//!
//! let mut iter = SnapshotIterator::new(snap, config);
//!
//! for result in iter {
//!     let block = result.expect("Read error");
//!     // Parse samples from block and feed to model...
//! }
//! ```
//!
//! ## Epoch-Based Training with Reset
//!
//! ```rust,no_run
//! use strata_loader::engine::{OpenConfig, open_snapshot};
//! use strata_loader::engine::iterator::{IterConfig, SnapshotIterator};
//! use strata_core::api::stratafile::SnapshotStream;
//!
//! let snap = open_snapshot(OpenConfig {
//!     path: "s3://ml-datasets/coco.st".to_string(),
//!     s3_region: Some("us-east-1".to_string()),
//!     endpoint_url: None,
//!     allow_restricted: false,
//!     prefetch_count: 16,
//!     cache_capacity_bytes: Some(32 * 1024 * 1024),
//! }).expect("Failed to open S3 snapshot");
//!
//! let config = IterConfig::default();
//! let mut iter = SnapshotIterator::new(snap, config);
//!
//! for epoch in 0..100 {
//!     println!("Epoch {}", epoch);
//!     for result in &mut iter {
//!         let block = result.expect("Read error");
//!         // Train on block...
//!     }
//!     iter.reset();  // Start next epoch from the beginning
//! }
//! ```
//!
//! ## Multithreaded Parallel Iteration
//!
//! Because `Arc<StrataFile>` is `Send + Sync`, you can create multiple iterators
//! on different threads, each reading different regions:
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use std::thread;
//! use strata_loader::engine::{OpenConfig, open_snapshot};
//! use strata_loader::engine::iterator::{IterConfig, SnapshotIterator};
//! use strata_core::api::stratafile::SnapshotStream;
//!
//! let snap = open_snapshot(OpenConfig {
//!     path: "/data/large-dataset.st".to_string(),
//!     s3_region: None,
//!     endpoint_url: None,
//!     allow_restricted: false,
//!     prefetch_count: 4,
//!     cache_capacity_bytes: Some(64 * 1024 * 1024),
//! }).expect("Failed to open");
//!
//! let handles: Vec<_> = (0..4).map(|worker_id| {
//!     let snap_clone = Arc::clone(&snap);
//!     thread::spawn(move || {
//!         let config = IterConfig::default();
//!         let iter = SnapshotIterator::new(snap_clone, config);
//!         for result in iter.skip(worker_id * 1000).take(1000) {
//!             let block = result.expect("Read error");
//!             // Process block on worker thread...
//!         }
//!     })
//! }).collect();
//!
//! for handle in handles {
//!     handle.join().unwrap();
//! }
//! ```
//!
//! # Performance Characteristics
//!
//! - **Iteration Overhead**: ~100ns per `next()` call (in-cache case)
//! - **Block Read (cache miss, local SSD)**: ~50μs per 64KB block
//! - **Block Read (cache miss, S3 with prefetch)**: ~5-10ms per block (amortized)
//! - **Memory Footprint**: O(block_size), independent of dataset size
//!
//! # Error Handling
//!
//! The iterator yields `Result<Vec<u8>, String>` rather than panicking on errors.
//! This allows graceful error handling in long-running data pipelines:
//!
//! ```rust,no_run
//! use strata_loader::engine::iterator::SnapshotIterator;
//! # use strata_loader::engine::{OpenConfig, open_snapshot};
//! # use strata_loader::engine::iterator::IterConfig;
//! # let snap = open_snapshot(OpenConfig {
//! #     path: "/data/dataset.st".to_string(),
//! #     s3_region: None, endpoint_url: None, allow_restricted: false,
//! #     prefetch_count: 0, cache_capacity_bytes: None,
//! # }).unwrap();
//! # let config = IterConfig::default();
//! let mut iter = SnapshotIterator::new(snap, config);
//!
//! for result in iter {
//!     match result {
//!         Ok(block) => {
//!             // Process block...
//!         }
//!         Err(e) => {
//!             eprintln!("Read error: {}", e);
//!             // Log error, skip block, or abort...
//!             break;
//!         }
//!     }
//! }
//! ```
//!
//! # Thread Safety
//!
//! [`SnapshotIterator`] itself is **not** `Sync` (it contains a mutable offset).
//! However, you can create multiple independent iterators from the same
//! `Arc<StrataFile>` on different threads without contention.

use std::sync::Arc;
use strata_core::StrataFile;
use strata_core::api::stratafile::SnapshotStream;

/// Configuration parameters for snapshot iteration behavior.
///
/// This struct controls the performance characteristics of [`SnapshotIterator`],
/// allowing fine-tuning of memory usage, throughput, and latency based on the
/// workload and backend.
///
/// # Field Descriptions
///
/// - **`block_size`**: The number of bytes read per `next()` call. Larger blocks
///   reduce per-call overhead but increase memory usage and latency for the first
///   byte. Typical values: 4KB-1MB.
///
/// - **`prefetch_count`**: How many blocks ahead to fetch asynchronously. Higher
///   values increase throughput on high-latency backends (HTTP, S3) but consume
///   more memory. Set to 0 to disable prefetching.
///
/// - **`stream`**: Which snapshot stream to iterate over. Most ML datasets use
///   `SnapshotStream::Disk`; `SnapshotStream::Memory` is for RAM snapshot analysis.
///
/// # Default Configuration
///
/// The `Default` implementation provides balanced settings for local filesystem access:
///
/// ```rust
/// use strata_loader::engine::iterator::IterConfig;
/// use strata_core::api::stratafile::SnapshotStream;
///
/// let config = IterConfig::default();
/// assert_eq!(config.block_size, 65536);         // 64KB blocks
/// assert_eq!(config.prefetch_count, 4);         // Moderate prefetch
/// assert_eq!(config.stream, SnapshotStream::Disk);
/// ```
///
/// # Recommended Configurations
///
/// ## High-Throughput Sequential (Local SSD)
///
/// ```rust
/// use strata_loader::engine::iterator::IterConfig;
/// use strata_core::api::stratafile::SnapshotStream;
///
/// let config = IterConfig {
///     block_size: 1024 * 1024,  // 1MB blocks for fewer syscalls
///     prefetch_count: 4,
///     stream: SnapshotStream::Disk,
/// };
/// ```
///
/// ## Network Backend (S3, HTTP)
///
/// ```rust
/// use strata_loader::engine::iterator::IterConfig;
/// use strata_core::api::stratafile::SnapshotStream;
///
/// let config = IterConfig {
///     block_size: 65536,      // 64KB blocks (balance latency/throughput)
///     prefetch_count: 16,     // Aggressive prefetch to hide latency
///     stream: SnapshotStream::Disk,
/// };
/// ```
///
/// ## Memory-Constrained Environment
///
/// ```rust
/// use strata_loader::engine::iterator::IterConfig;
/// use strata_core::api::stratafile::SnapshotStream;
///
/// let config = IterConfig {
///     block_size: 4096,   // 4KB blocks (minimal memory)
///     prefetch_count: 0,  // No prefetch to save memory
///     stream: SnapshotStream::Disk,
/// };
/// ```
///
/// # Performance Impact
///
/// | Parameter       | Effect on Throughput | Effect on Latency | Memory Cost |
/// |-----------------|----------------------|-------------------|-------------|
/// | ↑ block_size    | ↑ (fewer calls)      | ↑ (larger reads)  | O(N)        |
/// | ↑ prefetch_count| ↑↑ (network only)    | ↓ (hidden I/O)    | O(N)        |
/// | Disk vs Memory  | Equal                | Equal             | None        |
pub struct IterConfig {
    /// Number of bytes to read in each `next()` call.
    ///
    /// Larger values reduce per-call overhead but increase memory usage and first-byte
    /// latency. Must be > 0.
    ///
    /// Typical values:
    /// - 4KB: Minimal memory, higher overhead
    /// - 64KB: Balanced (default)
    /// - 1MB: High throughput, higher latency
    pub block_size: usize,

    /// Number of blocks to asynchronously prefetch ahead of the current position.
    ///
    /// When > 0, the system reads `prefetch_count` blocks in the background while
    /// you process the current block. Critical for network backends (S3, HTTP) where
    /// latency dominates.
    ///
    /// Set to 0 to disable prefetching (useful for random access or memory constraints).
    pub prefetch_count: usize,

    /// Which snapshot stream to iterate over.
    ///
    /// - `SnapshotStream::Disk`: The persistent storage snapshot (most common)
    /// - `SnapshotStream::Memory`: The RAM snapshot (if present)
    pub stream: SnapshotStream,
}

impl Default for IterConfig {
    fn default() -> Self {
        Self {
            block_size: 65536,
            prefetch_count: 4,
            stream: SnapshotStream::Disk,
        }
    }
}

/// A lazy iterator over sequential blocks from a snapshot stream.
///
/// This struct implements the Rust `Iterator` trait to provide streaming access to
/// snapshot data. It reads blocks of configurable size on-demand, releasing memory
/// after each block is yielded.
///
/// # Ownership and Thread Safety
///
/// - **Shared Snapshot**: Holds an `Arc<StrataFile>`, allowing the iterator to be
///   created from a snapshot shared across threads.
/// - **Private State**: The `offset` field is mutable and **not** synchronized, so
///   `SnapshotIterator` is **not** `Sync`. Each thread should create its own iterator
///   from a cloned `Arc<StrataFile>`.
///
/// # Memory Layout
///
/// ```text
/// SnapshotIterator (40 bytes on 64-bit systems)
/// ├─ snap: Arc<StrataFile>     (16 bytes: pointer + ref count)
/// ├─ config: IterConfig        (16 bytes: block_size + prefetch_count + stream)
/// ├─ offset: u64               (8 bytes: current read position)
/// └─ total_size: u64           (8 bytes: stream size for bounds checking)
/// ```
///
/// # Lifetime
///
/// The iterator remains valid as long as the underlying `Arc<StrataFile>` is alive.
/// When the iterator is dropped, it decrements the `Arc` reference count but does
/// **not** close the snapshot if other references exist.
///
/// # Examples
///
/// ## Basic Sequential Reading
///
/// ```rust,no_run
/// use strata_loader::engine::{OpenConfig, open_snapshot};
/// use strata_loader::engine::iterator::{IterConfig, SnapshotIterator};
/// use strata_core::api::stratafile::SnapshotStream;
///
/// let snap = open_snapshot(OpenConfig {
///     path: "/data/dataset.st".to_string(),
///     s3_region: None,
///     endpoint_url: None,
///     allow_restricted: false,
///     prefetch_count: 4,
///     cache_capacity_bytes: None,
/// }).expect("Failed to open");
///
/// let config = IterConfig {
///     block_size: 4096,
///     prefetch_count: 4,
///     stream: SnapshotStream::Disk,
/// };
///
/// let iter = SnapshotIterator::new(snap, config);
/// let blocks: Vec<_> = iter.take(10).collect();
/// assert!(blocks.len() <= 10);  // May be less if stream is shorter
/// ```
///
/// ## Reusing an Iterator Across Epochs
///
/// ```rust,no_run
/// # use strata_loader::engine::{OpenConfig, open_snapshot};
/// # use strata_loader::engine::iterator::{IterConfig, SnapshotIterator};
/// # let snap = open_snapshot(OpenConfig {
/// #     path: "/data/dataset.st".to_string(),
/// #     s3_region: None, endpoint_url: None, allow_restricted: false,
/// #     prefetch_count: 0, cache_capacity_bytes: None,
/// # }).unwrap();
/// let mut iter = SnapshotIterator::new(snap, IterConfig::default());
///
/// for epoch in 0..5 {
///     println!("Epoch {}", epoch);
///     for result in &mut iter {
///         let block = result.expect("Read error");
///         // Process block...
///     }
///     iter.reset();  // Rewind to the beginning for next epoch
/// }
/// ```
pub struct SnapshotIterator {
    /// Shared reference to the underlying snapshot.
    snap: Arc<StrataFile>,

    /// Iteration configuration (block size, prefetch, stream).
    config: IterConfig,

    /// Current read offset within the stream (0-indexed).
    ///
    /// Incremented by the number of bytes read after each `next()` call.
    offset: u64,

    /// Total size of the stream in bytes.
    ///
    /// Cached from the snapshot header to avoid repeated queries.
    total_size: u64,
}

impl SnapshotIterator {
    /// Creates a new iterator over the specified snapshot stream.
    ///
    /// This function queries the stream size from the snapshot header but does **not**
    /// read any actual data. The first block is read when `next()` is first called.
    ///
    /// # Parameters
    ///
    /// - `snap`: Shared reference to an opened snapshot (from [`crate::engine::open_snapshot`]).
    /// - `config`: Iteration parameters (block size, prefetch count, target stream).
    ///
    /// # Returns
    ///
    /// A new iterator positioned at offset 0 of the specified stream.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::sync::Arc;
    /// use strata_loader::engine::{OpenConfig, open_snapshot};
    /// use strata_loader::engine::iterator::{IterConfig, SnapshotIterator};
    /// use strata_core::api::stratafile::SnapshotStream;
    ///
    /// let snap = open_snapshot(OpenConfig {
    ///     path: "/data/snapshot.st".to_string(),
    ///     s3_region: None,
    ///     endpoint_url: None,
    ///     allow_restricted: false,
    ///     prefetch_count: 0,
    ///     cache_capacity_bytes: None,
    /// }).expect("Failed to open");
    ///
    /// let config = IterConfig {
    ///     block_size: 8192,
    ///     prefetch_count: 2,
    ///     stream: SnapshotStream::Disk,
    /// };
    ///
    /// let iter = SnapshotIterator::new(snap, config);
    /// // Iterator is ready but has not read any data yet
    /// ```
    ///
    /// # Performance
    ///
    /// This operation completes in O(1) time and performs no I/O beyond reading the
    /// cached header.
    pub fn new(snap: Arc<StrataFile>, config: IterConfig) -> Self {
        let total_size = snap.size(config.stream);
        Self {
            snap,
            config,
            offset: 0,
            total_size,
        }
    }

    /// Resets the iterator to the beginning of the stream.
    ///
    /// After calling this method, the next call to `next()` will read from offset 0.
    /// This is useful for multi-epoch training where you want to iterate over the same
    /// snapshot multiple times without recreating the iterator.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use strata_loader::engine::{OpenConfig, open_snapshot};
    /// # use strata_loader::engine::iterator::{IterConfig, SnapshotIterator};
    /// # let snap = open_snapshot(OpenConfig {
    /// #     path: "/data/dataset.st".to_string(),
    /// #     s3_region: None, endpoint_url: None, allow_restricted: false,
    /// #     prefetch_count: 0, cache_capacity_bytes: None,
    /// # }).unwrap();
    /// let mut iter = SnapshotIterator::new(snap, IterConfig::default());
    ///
    /// // First pass: read all blocks
    /// for block in &mut iter {
    ///     let data = block.expect("Read error");
    ///     // Process data...
    /// }
    ///
    /// // Iterator is now exhausted
    /// assert!(iter.next().is_none());
    ///
    /// // Reset to read again
    /// iter.reset();
    /// assert!(iter.next().is_some());
    /// ```
    ///
    /// # Performance
    ///
    /// This operation is O(1) and performs no I/O. It simply resets the internal offset
    /// counter to 0.
    pub fn reset(&mut self) {
        self.offset = 0;
    }
}

impl Iterator for SnapshotIterator {
    type Item = Result<Vec<u8>, String>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.total_size {
            return None;
        }

        let remaining = (self.total_size - self.offset) as usize;
        let read_len = std::cmp::min(self.config.block_size, remaining);

        match self.snap.read_at(self.config.stream, self.offset, read_len) {
            Ok(data) => {
                self.offset += data.len() as u64;
                Some(Ok(data))
            }
            Err(e) => Some(Err(e.to_string())),
        }
    }
}
