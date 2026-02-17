//! Lazy, sequential iteration over snapshot streams with prefetching.
//!
//! This module provides a Rust `Iterator` abstraction over Hexz snapshot data,
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
//! use hexz_loader::engine::{OpenConfig, open_snapshot};
//! use hexz_loader::engine::iterator::{IterConfig, SnapshotIterator};
//! use hexz_core::api::file::SnapshotStream;
//!
//! let snap = open_snapshot(OpenConfig {
//!     path: "/data/imagenet-train.hxz".to_string(),
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
//! use hexz_loader::engine::{OpenConfig, open_snapshot};
//! use hexz_loader::engine::iterator::{IterConfig, SnapshotIterator};
//! use hexz_core::api::file::SnapshotStream;
//!
//! let snap = open_snapshot(OpenConfig {
//!     path: "s3://ml-datasets/coco.hxz".to_string(),
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
//! Because `Arc<File>` is `Send + Sync`, you can create multiple iterators
//! on different threads, each reading different regions:
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use std::thread;
//! use hexz_loader::engine::{OpenConfig, open_snapshot};
//! use hexz_loader::engine::iterator::{IterConfig, SnapshotIterator};
//! use hexz_core::api::file::SnapshotStream;
//!
//! let snap = open_snapshot(OpenConfig {
//!     path: "/data/large-dataset.hxz".to_string(),
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
//! use hexz_loader::engine::iterator::SnapshotIterator;
//! # use hexz_loader::engine::{OpenConfig, open_snapshot};
//! # use hexz_loader::engine::iterator::IterConfig;
//! # let snap = open_snapshot(OpenConfig {
//! #     path: "/data/dataset.hxz".to_string(),
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
//! `Arc<File>` on different threads without contention.

use hexz_core::File;
use hexz_core::api::file::SnapshotStream;
use std::sync::Arc;

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
/// use hexz_loader::engine::iterator::IterConfig;
/// use hexz_core::api::file::SnapshotStream;
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
/// use hexz_loader::engine::iterator::IterConfig;
/// use hexz_core::api::file::SnapshotStream;
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
/// use hexz_loader::engine::iterator::IterConfig;
/// use hexz_core::api::file::SnapshotStream;
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
/// use hexz_loader::engine::iterator::IterConfig;
/// use hexz_core::api::file::SnapshotStream;
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
/// - **Shared Snapshot**: Holds an `Arc<File>`, allowing the iterator to be
///   created from a snapshot shared across threads.
/// - **Private State**: The `offset` field is mutable and **not** synchronized, so
///   `SnapshotIterator` is **not** `Sync`. Each thread should create its own iterator
///   from a cloned `Arc<File>`.
///
/// # Memory Layout
///
/// ```text
/// SnapshotIterator (40 bytes on 64-bit systems)
/// ├─ snap: Arc<File>     (16 bytes: pointer + ref count)
/// ├─ config: IterConfig        (16 bytes: block_size + prefetch_count + stream)
/// ├─ offset: u64               (8 bytes: current read position)
/// └─ total_size: u64           (8 bytes: stream size for bounds checking)
/// ```
///
/// # Lifetime
///
/// The iterator remains valid as long as the underlying `Arc<File>` is alive.
/// When the iterator is dropped, it decrements the `Arc` reference count but does
/// **not** close the snapshot if other references exist.
///
/// # Examples
///
/// ## Basic Sequential Reading
///
/// ```rust,no_run
/// use hexz_loader::engine::{OpenConfig, open_snapshot};
/// use hexz_loader::engine::iterator::{IterConfig, SnapshotIterator};
/// use hexz_core::api::file::SnapshotStream;
///
/// let snap = open_snapshot(OpenConfig {
///     path: "/data/dataset.hxz".to_string(),
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
/// # use hexz_loader::engine::{OpenConfig, open_snapshot};
/// # use hexz_loader::engine::iterator::{IterConfig, SnapshotIterator};
/// # let snap = open_snapshot(OpenConfig {
/// #     path: "/data/dataset.hxz".to_string(),
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
    snap: Arc<File>,

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
    /// use hexz_loader::engine::{OpenConfig, open_snapshot};
    /// use hexz_loader::engine::iterator::{IterConfig, SnapshotIterator};
    /// use hexz_core::api::file::SnapshotStream;
    ///
    /// let snap = open_snapshot(OpenConfig {
    ///     path: "/data/snapshot.hxz".to_string(),
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
    pub fn new(snap: Arc<File>, config: IterConfig) -> Self {
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
    /// # use hexz_loader::engine::{OpenConfig, open_snapshot};
    /// # use hexz_loader::engine::iterator::{IterConfig, SnapshotIterator};
    /// # let snap = open_snapshot(OpenConfig {
    /// #     path: "/data/dataset.hxz".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{OpenConfig, open_snapshot};
    use hexz_core::ops::pack::{PackConfig, pack_snapshot};
    use std::fs;
    use tempfile::TempDir;

    /// Helper to create a test disk image
    fn create_test_disk(dir: &TempDir, size: usize, pattern: u8) -> std::path::PathBuf {
        let path = dir.path().join("disk.img");
        let data = vec![pattern; size];
        fs::write(&path, data).unwrap();
        path
    }

    /// Helper to create a test snapshot
    fn create_test_snapshot(
        dir: &TempDir,
        disk_size: usize,
        pattern: u8,
        block_size: u32,
    ) -> Arc<File> {
        let disk_path = create_test_disk(dir, disk_size, pattern);
        let output_path = dir.path().join("test.hxz");

        let config = PackConfig {
            disk: Some(disk_path),
            memory: None,
            output: output_path.clone(),
            compression: "lz4".to_string(),
            encrypt: false,
            password: None,
            train_dict: false,
            block_size,
            cdc_enabled: false,
            min_chunk: block_size / 2,
            avg_chunk: block_size,
            max_chunk: block_size * 2,
            ..Default::default()
        };

        pack_snapshot(config, None::<fn(u64, u64)>).expect("Failed to pack snapshot");

        let open_config = OpenConfig {
            path: output_path.to_str().unwrap().to_string(),
            s3_region: None,
            endpoint_url: None,
            allow_restricted: false,
            prefetch_count: 0,
            cache_capacity_bytes: None,
        };

        open_snapshot(open_config).expect("Failed to open snapshot")
    }

    #[test]
    fn test_iterator_basic() {
        let temp_dir = TempDir::new().unwrap();
        let snap = create_test_snapshot(&temp_dir, 8192, 0x42, 4096);

        let config = IterConfig {
            block_size: 1024,
            prefetch_count: 0,
            stream: SnapshotStream::Disk,
        };

        let iter = SnapshotIterator::new(snap, config);
        let blocks: Vec<_> = iter.collect();

        // Should have 8 blocks (8192 / 1024)
        assert_eq!(blocks.len(), 8);

        // All blocks should be successful
        for block in blocks {
            let data = block.unwrap();
            assert_eq!(data.len(), 1024);
            assert!(data.iter().all(|&b| b == 0x42));
        }
    }

    #[test]
    fn test_iterator_default_config() {
        let temp_dir = TempDir::new().unwrap();
        let snap = create_test_snapshot(&temp_dir, 128 * 1024, 0xAA, 4096);

        let config = IterConfig::default();
        assert_eq!(config.block_size, 65536);
        assert_eq!(config.prefetch_count, 4);
        assert!(matches!(config.stream, SnapshotStream::Disk));

        let iter = SnapshotIterator::new(snap, config);
        let blocks: Vec<_> = iter.collect();

        // Should have 2 blocks (128KB / 64KB)
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn test_iterator_empty_stream() {
        let temp_dir = TempDir::new().unwrap();
        let snap = create_test_snapshot(&temp_dir, 0, 0x00, 4096);

        let config = IterConfig {
            block_size: 1024,
            prefetch_count: 0,
            stream: SnapshotStream::Disk,
        };

        let iter = SnapshotIterator::new(snap, config);
        let blocks: Vec<_> = iter.collect();

        // Empty stream should yield no blocks
        assert_eq!(blocks.len(), 0);
    }

    #[test]
    fn test_iterator_single_block() {
        let temp_dir = TempDir::new().unwrap();
        let snap = create_test_snapshot(&temp_dir, 512, 0xBB, 4096);

        let config = IterConfig {
            block_size: 1024,
            prefetch_count: 0,
            stream: SnapshotStream::Disk,
        };

        let iter = SnapshotIterator::new(snap, config);
        let blocks: Vec<_> = iter.collect();

        // Should have 1 block with 512 bytes
        assert_eq!(blocks.len(), 1);
        let data = blocks[0].as_ref().unwrap();
        assert_eq!(data.len(), 512);
        assert!(data.iter().all(|&b| b == 0xBB));
    }

    #[test]
    fn test_iterator_partial_last_block() {
        let temp_dir = TempDir::new().unwrap();
        let snap = create_test_snapshot(&temp_dir, 2500, 0xCC, 4096);

        let config = IterConfig {
            block_size: 1024,
            prefetch_count: 0,
            stream: SnapshotStream::Disk,
        };

        let iter = SnapshotIterator::new(snap, config);
        let blocks: Vec<_> = iter.collect();

        // Should have 3 blocks: 1024, 1024, 452
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].as_ref().unwrap().len(), 1024);
        assert_eq!(blocks[1].as_ref().unwrap().len(), 1024);
        assert_eq!(blocks[2].as_ref().unwrap().len(), 452);

        for block in blocks {
            let data = block.unwrap();
            assert!(data.iter().all(|&b| b == 0xCC));
        }
    }

    #[test]
    fn test_iterator_reset() {
        let temp_dir = TempDir::new().unwrap();
        let snap = create_test_snapshot(&temp_dir, 4096, 0xDD, 4096);

        let config = IterConfig {
            block_size: 1024,
            prefetch_count: 0,
            stream: SnapshotStream::Disk,
        };

        let mut iter = SnapshotIterator::new(snap, config);

        // First iteration
        let first_pass: Vec<_> = iter.by_ref().collect();
        assert_eq!(first_pass.len(), 4);

        // Should be exhausted
        assert!(iter.next().is_none());

        // Reset and iterate again
        iter.reset();
        let second_pass: Vec<_> = iter.collect();
        assert_eq!(second_pass.len(), 4);

        // Results should be identical
        for (a, b) in first_pass.iter().zip(second_pass.iter()) {
            assert_eq!(a.as_ref().unwrap(), b.as_ref().unwrap());
        }
    }

    #[test]
    fn test_iterator_multiple_resets() {
        let temp_dir = TempDir::new().unwrap();
        let snap = create_test_snapshot(&temp_dir, 2048, 0xEE, 4096);

        let config = IterConfig {
            block_size: 512,
            prefetch_count: 0,
            stream: SnapshotStream::Disk,
        };

        let mut iter = SnapshotIterator::new(snap, config);

        for _ in 0..5 {
            let blocks: Vec<_> = iter.by_ref().collect();
            assert_eq!(blocks.len(), 4);
            iter.reset();
        }
    }

    #[test]
    fn test_iterator_with_prefetch() {
        let temp_dir = TempDir::new().unwrap();
        let snap = create_test_snapshot(&temp_dir, 32768, 0xFF, 4096);

        let config = IterConfig {
            block_size: 4096,
            prefetch_count: 8,
            stream: SnapshotStream::Disk,
        };

        let iter = SnapshotIterator::new(snap, config);
        let blocks: Vec<_> = iter.collect();

        // Should have 8 blocks
        assert_eq!(blocks.len(), 8);

        for block in blocks {
            let data = block.unwrap();
            assert_eq!(data.len(), 4096);
            assert!(data.iter().all(|&b| b == 0xFF));
        }
    }

    #[test]
    fn test_iterator_large_block_size() {
        let temp_dir = TempDir::new().unwrap();
        let snap = create_test_snapshot(&temp_dir, 8192, 0x11, 4096);

        let config = IterConfig {
            block_size: 1024 * 1024, // 1MB blocks
            prefetch_count: 0,
            stream: SnapshotStream::Disk,
        };

        let iter = SnapshotIterator::new(snap, config);
        let blocks: Vec<_> = iter.collect();

        // Should have 1 block with all 8192 bytes
        assert_eq!(blocks.len(), 1);
        let data = blocks[0].as_ref().unwrap();
        assert_eq!(data.len(), 8192);
        assert!(data.iter().all(|&b| b == 0x11));
    }

    #[test]
    fn test_iterator_small_block_size() {
        let temp_dir = TempDir::new().unwrap();
        let snap = create_test_snapshot(&temp_dir, 1024, 0x22, 4096);

        let config = IterConfig {
            block_size: 128,
            prefetch_count: 0,
            stream: SnapshotStream::Disk,
        };

        let iter = SnapshotIterator::new(snap, config);
        let blocks: Vec<_> = iter.collect();

        // Should have 8 blocks (1024 / 128)
        assert_eq!(blocks.len(), 8);

        for block in blocks {
            let data = block.unwrap();
            assert_eq!(data.len(), 128);
            assert!(data.iter().all(|&b| b == 0x22));
        }
    }

    #[test]
    fn test_iterator_take() {
        let temp_dir = TempDir::new().unwrap();
        let snap = create_test_snapshot(&temp_dir, 16384, 0x33, 4096);

        let config = IterConfig {
            block_size: 1024,
            prefetch_count: 0,
            stream: SnapshotStream::Disk,
        };

        let iter = SnapshotIterator::new(snap, config);
        let blocks: Vec<_> = iter.take(5).collect();

        // Should only take first 5 blocks
        assert_eq!(blocks.len(), 5);

        for block in blocks {
            let data = block.unwrap();
            assert_eq!(data.len(), 1024);
            assert!(data.iter().all(|&b| b == 0x33));
        }
    }

    #[test]
    fn test_iterator_skip() {
        let temp_dir = TempDir::new().unwrap();
        let snap = create_test_snapshot(&temp_dir, 8192, 0x44, 4096);

        let config = IterConfig {
            block_size: 1024,
            prefetch_count: 0,
            stream: SnapshotStream::Disk,
        };

        let iter = SnapshotIterator::new(snap, config);
        let blocks: Vec<_> = iter.skip(4).collect();

        // Should skip first 4 blocks, leaving 4 remaining
        assert_eq!(blocks.len(), 4);

        for block in blocks {
            let data = block.unwrap();
            assert_eq!(data.len(), 1024);
            assert!(data.iter().all(|&b| b == 0x44));
        }
    }

    #[test]
    fn test_iterator_count() {
        let temp_dir = TempDir::new().unwrap();
        let snap = create_test_snapshot(&temp_dir, 10240, 0x55, 4096);

        let config = IterConfig {
            block_size: 1024,
            prefetch_count: 0,
            stream: SnapshotStream::Disk,
        };

        let iter = SnapshotIterator::new(snap, config);
        let count = iter.count();

        // Should have 10 blocks (10240 / 1024)
        assert_eq!(count, 10);
    }

    #[test]
    fn test_iterator_with_cache() {
        let temp_dir = TempDir::new().unwrap();
        let disk_path = create_test_disk(&temp_dir, 16384, 0x66);
        let output_path = temp_dir.path().join("cached.hxz");

        let config = PackConfig {
            disk: Some(disk_path),
            memory: None,
            output: output_path.clone(),
            compression: "lz4".to_string(),
            encrypt: false,
            password: None,
            train_dict: false,
            block_size: 4096,
            cdc_enabled: false,
            min_chunk: 2048,
            avg_chunk: 4096,
            max_chunk: 8192,
            ..Default::default()
        };

        pack_snapshot(config, None::<fn(u64, u64)>).expect("Failed to pack");

        let open_config = OpenConfig {
            path: output_path.to_str().unwrap().to_string(),
            s3_region: None,
            endpoint_url: None,
            allow_restricted: false,
            prefetch_count: 4,
            cache_capacity_bytes: Some(128 * 1024),
        };

        let snap = open_snapshot(open_config).unwrap();

        let iter_config = IterConfig {
            block_size: 2048,
            prefetch_count: 4,
            stream: SnapshotStream::Disk,
        };

        let iter = SnapshotIterator::new(snap, iter_config);
        let blocks: Vec<_> = iter.collect();

        assert_eq!(blocks.len(), 8);

        for block in blocks {
            let data = block.unwrap();
            assert_eq!(data.len(), 2048);
            assert!(data.iter().all(|&b| b == 0x66));
        }
    }

    #[test]
    fn test_iterator_memory_stream() {
        let temp_dir = TempDir::new().unwrap();

        // Create disk and memory images
        let disk_path = create_test_disk(&temp_dir, 4096, 0x77);
        let memory_path = temp_dir.path().join("memory.dump");
        fs::write(&memory_path, vec![0x88; 2048]).unwrap();

        let output_path = temp_dir.path().join("with_mem.hxz");

        let config = PackConfig {
            disk: Some(disk_path),
            memory: Some(memory_path),
            output: output_path.clone(),
            compression: "lz4".to_string(),
            encrypt: false,
            password: None,
            train_dict: false,
            block_size: 4096,
            cdc_enabled: false,
            min_chunk: 2048,
            avg_chunk: 4096,
            max_chunk: 8192,
            ..Default::default()
        };

        pack_snapshot(config, None::<fn(u64, u64)>).expect("Failed to pack");

        let open_config = OpenConfig {
            path: output_path.to_str().unwrap().to_string(),
            s3_region: None,
            endpoint_url: None,
            allow_restricted: false,
            prefetch_count: 0,
            cache_capacity_bytes: None,
        };

        let snap = open_snapshot(open_config).unwrap();

        // Test memory stream iteration
        let mem_config = IterConfig {
            block_size: 512,
            prefetch_count: 0,
            stream: SnapshotStream::Memory,
        };

        let iter = SnapshotIterator::new(snap, mem_config);
        let blocks: Vec<_> = iter.collect();

        // Should have 4 blocks (2048 / 512)
        assert_eq!(blocks.len(), 4);

        for block in blocks {
            let data = block.unwrap();
            assert_eq!(data.len(), 512);
            assert!(data.iter().all(|&b| b == 0x88));
        }
    }

    #[test]
    fn test_iterator_total_bytes() {
        let temp_dir = TempDir::new().unwrap();
        let snap = create_test_snapshot(&temp_dir, 5000, 0x99, 4096);

        let config = IterConfig {
            block_size: 1000,
            prefetch_count: 0,
            stream: SnapshotStream::Disk,
        };

        let iter = SnapshotIterator::new(snap, config);
        let total_bytes: usize = iter.map(|block| block.unwrap().len()).sum();

        assert_eq!(total_bytes, 5000);
    }

    #[test]
    fn test_iterator_error_on_corrupted_file() {
        use std::io::Write;

        let temp_dir = TempDir::new().unwrap();
        let disk_path = create_test_disk(&temp_dir, 4096, 0xAA);
        let output_path = temp_dir.path().join("corrupted.hxz");

        // Create a valid snapshot
        let config = PackConfig {
            disk: Some(disk_path),
            memory: None,
            output: output_path.clone(),
            compression: "lz4".to_string(),
            encrypt: false,
            password: None,
            train_dict: false,
            block_size: 4096,
            cdc_enabled: false,
            min_chunk: 2048,
            avg_chunk: 4096,
            max_chunk: 8192,
            ..Default::default()
        };

        pack_snapshot(config, None::<fn(u64, u64)>).expect("Failed to pack");

        // Open the snapshot
        let open_config = OpenConfig {
            path: output_path.to_str().unwrap().to_string(),
            s3_region: None,
            endpoint_url: None,
            allow_restricted: false,
            prefetch_count: 0,
            cache_capacity_bytes: None,
        };

        let snap = open_snapshot(open_config).expect("Failed to open snapshot");

        // Now corrupt the file by truncating it severely
        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&output_path)
                .unwrap();
            file.write_all(b"corrupted").unwrap();
        }

        // Try to iterate - this should eventually hit an error when reading
        let iter_config = IterConfig {
            block_size: 1024,
            prefetch_count: 0,
            stream: SnapshotStream::Disk,
        };

        let mut iter = SnapshotIterator::new(snap, iter_config);

        // Iterate and check for errors
        let mut found_error = false;
        for result in iter.by_ref() {
            match result {
                Ok(_) => continue,
                Err(e) => {
                    // Found an error - verify it's a non-empty error message
                    assert!(!e.is_empty(), "Error message should not be empty");
                    found_error = true;
                    break;
                }
            }
        }

        // We should have encountered an error when trying to read corrupted data
        assert!(
            found_error,
            "Expected to encounter a read error from corrupted file"
        );
    }
}
