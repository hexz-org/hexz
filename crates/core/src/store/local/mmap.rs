//! Memory-mapped file storage backend.
//!
//! This module implements the `StorageBackend` trait using `mmap(2)` to map the
//! entire snapshot file into the process's virtual address space. This approach
//! delegates paging, caching, and prefetching to the kernel's virtual memory
//! subsystem, often providing superior performance for random-access workloads
//! compared to traditional file I/O.
//!
//! # Architecture
//!
//! The [`MmapBackend`] maps the entire file into memory using `MAP_PRIVATE` and
//! `PROT_READ` flags. The mapping is read-only and copy-on-write (though no writes
//! occur). The `Mmap` handle is wrapped in `Arc<Mmap>` to enable thread-safe
//! shared access across multiple readers.
//!
//! Key characteristics:
//! - **Zero-copy reads**: Data is accessed directly from mapped pages
//! - **Lazy loading**: Pages are only loaded from disk when accessed (demand paging)
//! - **Transparent caching**: The OS page cache automatically retains hot pages
//! - **Automatic prefetch**: Sequential access patterns trigger kernel read-ahead
//!
//! # Safety Considerations
//!
//! Memory mapping is inherently unsafe because:
//! - **File modification**: If another process truncates or modifies the file while
//!   mapped, accessing those pages can trigger `SIGBUS` (segmentation fault)
//! - **Race conditions**: Concurrent file modifications can cause undefined behavior
//!
//! This backend assumes **immutable snapshot semantics**: the file must not be
//! modified by any process while the backend is active. Violating this assumption
//! can cause process crashes or data corruption.
//!
//! # Thread Safety
//!
//! The backend is fully thread-safe (`Send + Sync`):
//! - The `Mmap` is wrapped in `Arc` for safe shared ownership
//! - Reads are lock-free (simple `memcpy` from mapped pages)
//! - The kernel handles concurrent page faults transparently
//!
//! Multiple threads can safely read from the same backend simultaneously without
//! coordination or contention.
//!
//! # Performance Characteristics
//!
//! - **Latency (resident pages)**: 1-5µs (direct memory access)
//! - **Latency (non-resident pages)**: 5-50µs (page fault + disk I/O)
//! - **Throughput**: Up to memory bandwidth (~10-50GB/s for resident data)
//! - **CPU overhead**: Zero-copy for resident pages, page fault handler for cold pages
//! - **Memory overhead**: OS transparently manages resident set size; can evict under pressure
//!
//! # When to Use This Backend
//!
//! Prefer [`MmapBackend`] over [`FileBackend`](super::file::FileBackend) when:
//! - Working with small to medium files (<1GB) that fit mostly in RAM
//! - Access patterns exhibit strong spatial locality (e.g., scanning index structures)
//! - You want to leverage OS-level prefetching and caching heuristics
//! - Minimizing CPU overhead is critical (zero-copy access)
//!
//! Avoid [`MmapBackend`] when:
//! - Files are very large (>10GB) with sparse access (wastes address space)
//! - Running in restricted environments where `mmap(2)` is disabled
//! - Profiling shows excessive page faults or thrashing
//! - You need explicit control over buffering and I/O scheduling
//!
//! # Platform Support
//!
//! This backend uses the `memmap2` crate, which supports:
//! - **Linux**: Full support via `mmap(2)`
//! - **macOS**: Full support via `mmap(2)`
//! - **Windows**: Supported via `MapViewOfFile` (not tested in this module)
//!
//! # Error Handling
//!
//! Errors can occur during:
//! - **Construction**: File open failure, insufficient address space, `mmap(2)` failure
//! - **Reads**: Out-of-bounds access (checked in software)
//!
//! Note: If the mapped file is modified externally, the kernel may deliver `SIGBUS`,
//! which this code cannot catch. Always ensure files are immutable.
//!
//! # Examples
//!
//! ```no_run
//! use strata_core::store::local::MmapBackend;
//! use strata_core::store::StorageBackend;
//! use std::path::Path;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Open and map a snapshot file
//! let backend = MmapBackend::new(Path::new("/data/snapshot.st"))?;
//!
//! // Read 4KB starting at offset 8192
//! let data = backend.read_exact(8192, 4096)?;
//! assert_eq!(data.len(), 4096);
//!
//! // Concurrent reads from multiple threads
//! let backend = std::sync::Arc::new(backend);
//! let handles: Vec<_> = (0..4)
//!     .map(|i| {
//!         let b = backend.clone();
//!         std::thread::spawn(move || {
//!             b.read_exact(i * 1024, 1024)
//!         })
//!     })
//!     .collect();
//!
//! for handle in handles {
//!     let result = handle.join().unwrap()?;
//!     assert_eq!(result.len(), 1024);
//! }
//! # Ok(())
//! # }
//! ```

use crate::store::StorageBackend;
use bytes::Bytes;
use memmap2::Mmap;
use std::fs::File;
use std::sync::Arc;
use strata_common::Result;

/// A storage backend backed by a memory-mapped file.
///
/// This struct holds a reference to the memory map. Accessing data is as efficient
/// as a memory copy (`memcpy`), avoiding the explicit system call overhead of
/// `read` or `pread`. The OS handles paging data in from disk as needed.
#[derive(Debug)]
pub struct MmapBackend {
    /// The memory map, wrapped in an `Arc` for thread-safe shared ownership.
    map: Arc<Mmap>,
    /// The total size of the mapped file in bytes.
    len: u64,
}

impl MmapBackend {
    /// Opens a snapshot file and maps it into the process address space.
    ///
    /// This constructor performs three operations:
    /// 1. Opens the file at `path` in read-only mode
    /// 2. Queries the file size via `fstat(2)`
    /// 3. Creates a read-only memory mapping (`MAP_PRIVATE | PROT_READ`)
    ///
    /// The mapping is private (copy-on-write), but since no writes occur, it
    /// effectively shares the underlying page cache with other processes reading
    /// the same file.
    ///
    /// # Parameters
    ///
    /// - `path`: Filesystem path to the snapshot file (absolute or relative)
    ///
    /// # Returns
    ///
    /// - `Ok(MmapBackend)`: Successfully mapped and initialized
    /// - `Err(StrataError::Io)`: If the file cannot be opened or mapped
    ///
    /// # Errors
    ///
    /// Common error conditions:
    /// - **File not found** (`ENOENT`): Path does not exist
    /// - **Permission denied** (`EACCES`): Insufficient permissions to read file
    /// - **Out of memory** (`ENOMEM`): Insufficient virtual address space (rare on 64-bit)
    /// - **Invalid file**: Cannot map special files (e.g., `/dev/null`, pipes, sockets)
    ///
    /// # Safety
    ///
    /// This function uses `unsafe { Mmap::map(&file) }` internally. The safety
    /// invariant is that the mapped file must not be modified or truncated by any
    /// process while the mapping exists. Violating this invariant can cause:
    /// - **SIGBUS**: Accessing pages corresponding to truncated regions
    /// - **Data races**: Undefined behavior if file contents change during reads
    ///
    /// This backend enforces immutable snapshot semantics: the caller must ensure
    /// the file is not modified after construction.
    ///
    /// # Performance
    ///
    /// The mapping operation is lazy. This constructor only reserves virtual address
    /// space; no disk I/O occurs until pages are accessed. For a 1GB file:
    /// - Constructor latency: ~100µs (no I/O, just syscall overhead)
    /// - First read latency: 5-50µs per 4KB page (demand paging)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use strata_core::store::local::MmapBackend;
    /// use std::path::Path;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Absolute path
    /// let backend = MmapBackend::new(Path::new("/var/data/snapshot.st"))?;
    ///
    /// // Relative path
    /// let backend = MmapBackend::new(Path::new("./snapshots/test.st"))?;
    ///
    /// // Error handling
    /// match MmapBackend::new(Path::new("/nonexistent.st")) {
    ///     Ok(_) => println!("Success"),
    ///     Err(e) => eprintln!("Failed to map: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(path: &std::path::Path) -> Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        let map = unsafe { Mmap::map(&file)? };
        Ok(Self {
            map: Arc::new(map),
            len,
        })
    }
}

impl StorageBackend for MmapBackend {
    /// Reads exactly `len` bytes starting at `offset` from the mapped file.
    ///
    /// This method performs a `memcpy` from the mapped region into a `Bytes` buffer.
    /// If the requested pages are not resident in RAM, the CPU will trigger a page
    /// fault, and the kernel will transparently load the data from disk (demand paging).
    ///
    /// # Parameters
    ///
    /// - `offset`: Absolute byte offset from the start of the file (0-indexed)
    /// - `len`: Number of bytes to read (must not cause `offset + len` to exceed file size)
    ///
    /// # Returns
    ///
    /// - `Ok(Bytes)`: A buffer containing exactly `len` bytes of data
    /// - `Err(StrataError::Io)`: If the read exceeds file boundaries
    ///
    /// # Errors
    ///
    /// This method returns an error if:
    /// - **Out of bounds** (`ErrorKind::UnexpectedEof`): `offset + len > file_size`
    ///
    /// Note: If the mapped file is modified externally (violating immutability),
    /// accessing those pages may trigger `SIGBUS`, which cannot be caught by this code.
    ///
    /// # Performance
    ///
    /// - **Time complexity**: O(len) for `memcpy`, O(1) for bounds check
    /// - **Syscalls**: 0 if pages are resident, page fault handler if not resident
    /// - **Allocations**: 1 heap allocation of `len` bytes for the returned `Bytes`
    /// - **CPU overhead**: ~10-50 CPU cycles per byte for resident pages
    ///
    /// **Latency breakdown** (4KB read):
    /// - Resident pages: ~1-5µs (pure memory copy)
    /// - Non-resident pages: ~5-50µs (page fault + disk I/O)
    ///
    /// # Concurrency
    ///
    /// This method is safe to call concurrently from multiple threads. The kernel
    /// handles concurrent page faults transparently. Reads never block each other
    /// unless they trigger page faults for the same pages simultaneously (rare and
    /// efficiently handled by the kernel).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use strata_core::store::local::MmapBackend;
    /// use strata_core::store::StorageBackend;
    /// use std::path::Path;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = MmapBackend::new(Path::new("/data/snapshot.st"))?;
    ///
    /// // Read first 512 bytes (header)
    /// let header = backend.read_exact(0, 512)?;
    /// assert_eq!(header.len(), 512);
    ///
    /// // Read 4KB block at offset 1MB
    /// let block = backend.read_exact(1024 * 1024, 4096)?;
    /// assert_eq!(block.len(), 4096);
    ///
    /// // Error: reading beyond file boundary
    /// let file_size = backend.len();
    /// assert!(backend.read_exact(file_size, 1).is_err());
    /// # Ok(())
    /// # }
    /// ```
    fn read_exact(&self, offset: u64, len: usize) -> Result<Bytes> {
        let start = offset as usize;
        let end = start + len;

        if end > self.map.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Read out of bounds",
            )
            .into());
        }

        Ok(Bytes::copy_from_slice(&self.map[start..end]))
    }

    /// Returns the total mapped file size in bytes.
    ///
    /// This value is cached during construction via `File::metadata()` and reflects
    /// the file size at the time the mapping was created. The file is assumed to be
    /// immutable; if the file is truncated or extended externally, this value will
    /// not be updated and accessing changed regions may cause undefined behavior.
    ///
    /// # Returns
    ///
    /// The file size in bytes as of the time `MmapBackend::new()` was called.
    ///
    /// # Performance
    ///
    /// This method is a simple field access with no system calls (O(1)).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use strata_core::store::local::MmapBackend;
    /// use strata_core::store::StorageBackend;
    /// use std::path::Path;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = MmapBackend::new(Path::new("/data/snapshot.st"))?;
    /// let size = backend.len();
    /// println!("Snapshot size: {} bytes ({} MB)", size, size / 1024 / 1024);
    /// # Ok(())
    /// # }
    /// ```
    fn len(&self) -> u64 {
        self.len
    }
}
