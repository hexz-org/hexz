//! Local file system storage backends.
//!
//! This module provides two complementary implementations of the `StorageBackend`
//! trait for accessing snapshot files on the local filesystem:
//!
//! - `FileBackend`: Traditional file I/O using `pread(2)` system calls
//! - `MmapBackend`: Memory-mapped file access leveraging the OS page cache
//!
//! # Architecture
//!
//! Both backends provide thread-safe, random-access reads to immutable snapshot
//! files stored locally. They are optimized for different access patterns and
//! performance characteristics.
//!
//! # Choosing a Backend
//!
//! | Scenario | Recommended Backend | Rationale |
//! |----------|-------------------|-----------|
//! | Sequential scans | `FileBackend` | Lower kernel overhead, predictable I/O scheduling |
//! | Random access with locality | `MmapBackend` | OS page cache provides transparent prefetch and caching |
//! | Small files (<100MB) | `MmapBackend` | Entire file likely stays resident, near-zero overhead |
//! | Large files (>1GB) with sparse access | `FileBackend` | Avoids address space fragmentation, explicit control |
//! | Containers/restricted environments | `FileBackend` | No special kernel capabilities required |
//!
//! # Thread Safety
//!
//! Both backends are fully thread-safe (`Send + Sync`):
//! - `FileBackend` uses offset-based I/O (`pread`) that bypasses file cursor state
//! - `MmapBackend` wraps the memory map in `Arc<Mmap>` for safe shared access
//!
//! Multiple threads can safely read from the same backend instance concurrently
//! without locks or coordination.
//!
//! # Performance Characteristics
//!
//! ## FileBackend
//!
//! - **Latency**: ~5-50µs per read (depends on page cache state)
//! - **Throughput**: Up to disk/SSD bandwidth (~500MB/s HDD, 3-7GB/s SSD)
//! - **CPU overhead**: One syscall per read, kernel copies data to userspace
//! - **Memory overhead**: Only requested data is copied into process memory
//!
//! ## MmapBackend
//!
//! - **Latency**: ~1-10µs per read (direct memory access if resident)
//! - **Throughput**: Limited by memory bandwidth (~10-50GB/s for resident data)
//! - **CPU overhead**: Zero-copy for resident pages, page fault handler for non-resident
//! - **Memory overhead**: OS manages paging transparently, can evict under pressure
//!
//! # Examples
//!
//! ```no_run
//! use hexz_core::store::local::{FileBackend, MmapBackend};
//! use hexz_core::store::StorageBackend;
//! use std::path::Path;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let path = Path::new("/data/snapshot.hxz");
//!
//! // Traditional file I/O
//! let file_backend = FileBackend::new(path)?;
//! let data = file_backend.read_exact(0, 512)?;
//!
//! // Memory-mapped I/O
//! let mmap_backend = MmapBackend::new(path)?;
//! let data = mmap_backend.read_exact(0, 512)?;
//! # Ok(())
//! # }
//! ```

/// File-backed storage backend using `pread` for thread-safe reads.
///
/// Prefer `FileBackend` over `MmapBackend` when:
/// - Accessing large files (>1GB) with sparse access patterns
/// - Working in restricted environments (containers)
/// - Sequential scan workloads
pub mod file;

/// Memory-mapped file backend leveraging the OS page cache.
///
/// Prefer `MmapBackend` over `FileBackend` when:
/// - Random access with spatial locality
/// - Small files (<100MB)
/// - Workloads that benefit from OS page cache management
pub mod mmap;

pub use file::FileBackend;
pub use mmap::MmapBackend;
