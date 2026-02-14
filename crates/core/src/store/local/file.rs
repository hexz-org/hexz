//! Local file system storage backend using position-independent I/O.
//!
//! This module implements the `StorageBackend` trait for files on local filesystems
//! using standard POSIX `pread(2)` semantics. Unlike traditional file I/O that
//! maintains a stateful read cursor, this backend uses offset-based reads that
//! allow safe concurrent access from multiple threads without synchronization.
//!
//! # Architecture
//!
//! The [`FileBackend`] wraps a single `std::fs::File` handle and caches the file
//! size at construction. All reads use `FileExt::read_exact_at()`, which maps to
//! the `pread(2)` system call on Unix systems. This design provides:
//!
//! - **Zero-lock concurrency**: Multiple threads can read simultaneously without contention
//! - **Predictable I/O**: Explicit control over read sizes and offsets
//! - **Kernel buffering**: Leverages the OS page cache for repeated reads
//!
//! # Thread Safety
//!
//! The backend is fully thread-safe (`Send + Sync`) because:
//! - The underlying `File` handle does not maintain a mutable cursor position
//! - All reads are atomic with respect to the file offset parameter
//! - The cached `size` field is immutable after initialization
//!
//! No internal locking is required. Multiple threads calling `read_exact()` will
//! not interfere with each other.
//!
//! # Performance Characteristics
//!
//! - **Latency**: 5-50µs per read (varies with page cache hit rate)
//! - **Throughput**: Limited by storage device (500MB/s HDD, 3-7GB/s SSD)
//! - **CPU overhead**: One system call + one kernel-to-userspace copy per read
//! - **Memory overhead**: Only requested data is allocated; no mapping overhead
//!
//! # When to Use This Backend
//!
//! Prefer [`FileBackend`] over [`MmapBackend`](super::mmap::MmapBackend) when:
//! - Accessing large files (>1GB) with sparse, unpredictable read patterns
//! - Running in restricted environments where `mmap(2)` may be disabled
//! - Profiling shows memory-mapped I/O causes excessive page faults
//! - You need explicit control over read granularity and buffering
//!
//! # Error Handling
//!
//! All I/O errors are wrapped in `Error::Io`. Common failure modes:
//! - File not found or insufficient permissions (construction)
//! - Unexpected EOF when reading beyond file boundaries
//! - Storage device failures or filesystem corruption (rare)
//!
//! # Examples
//!
//! ```no_run
//! use hexz_core::store::local::FileBackend;
//! use hexz_core::store::StorageBackend;
//! use std::path::Path;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Open a snapshot file
//! let backend = FileBackend::new(Path::new("/data/snapshot.hxz"))?;
//!
//! // Read 4KB starting at offset 8192
//! let data = backend.read_exact(8192, 4096)?;
//! assert_eq!(data.len(), 4096);
//!
//! // Multiple threads can read concurrently
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
use bytes::{Bytes, BytesMut};
use hexz_common::Result;
use std::fs::File;
use std::os::unix::fs::FileExt;

/// A storage backend implementation backed by a local file.
///
/// This struct wraps a standard `std::fs::File` handle. It provides thread-safe
/// access to the underlying file data by utilizing system calls that accept
/// an explicit offset, thereby bypassing the stateful file pointer. This design
/// eliminates the need for a `Mutex` around the file handle during read operations.
#[derive(Debug)]
pub struct FileBackend {
    /// The underlying operating system file handle.
    inner: File,
    /// The total size of the file in bytes, cached at initialization.
    size: u64,
}

impl FileBackend {
    /// Opens a snapshot file and prepares it for concurrent reads.
    ///
    /// This constructor performs two operations:
    /// 1. Opens the file at `path` in read-only mode (`O_RDONLY`)
    /// 2. Queries file metadata via `fstat(2)` to cache the file size
    ///
    /// The file size is cached to avoid repeated `stat` system calls. The backend
    /// assumes the file is immutable (snapshot semantics) and will not change
    /// size during its lifetime.
    ///
    /// # Parameters
    ///
    /// - `path`: Filesystem path to the snapshot file (absolute or relative)
    ///
    /// # Returns
    ///
    /// - `Ok(FileBackend)`: Successfully opened and initialized
    /// - `Err(Error::Io)`: If the file cannot be opened or metadata cannot be read
    ///
    /// # Errors
    ///
    /// Common error conditions:
    /// - **File not found** (`ENOENT`): Path does not exist
    /// - **Permission denied** (`EACCES`): Insufficient permissions to read file
    /// - **Invalid path** (`ENOTDIR`): A path component is not a directory
    /// - **Device errors**: Disk I/O failure during open or stat
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use hexz_core::store::local::FileBackend;
    /// use std::path::Path;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Absolute path
    /// let backend = FileBackend::new(Path::new("/var/data/snapshot.hxz"))?;
    ///
    /// // Relative path
    /// let backend = FileBackend::new(Path::new("./snapshots/test.hxz"))?;
    ///
    /// // Error handling
    /// match FileBackend::new(Path::new("/nonexistent.hxz")) {
    ///     Ok(_) => println!("Success"),
    ///     Err(e) => eprintln!("Failed to open: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(path: &std::path::Path) -> Result<Self> {
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        Ok(Self {
            inner: file,
            size: metadata.len(),
        })
    }
}

impl StorageBackend for FileBackend {
    /// Reads exactly `len` bytes starting at `offset` using position-independent I/O.
    ///
    /// This method uses `pread(2)` semantics (via `FileExt::read_exact_at`) to read
    /// data at an explicit offset without modifying any file descriptor state. This
    /// enables safe concurrent reads from multiple threads without coordination.
    ///
    /// # Parameters
    ///
    /// - `offset`: Absolute byte offset from the start of the file (0-indexed)
    /// - `len`: Number of bytes to read (must not cause `offset + len` to exceed file size)
    ///
    /// # Returns
    ///
    /// - `Ok(Bytes)`: A buffer containing exactly `len` bytes of data
    /// - `Err(Error::Io)`: If the read fails or reaches unexpected EOF
    ///
    /// # Errors
    ///
    /// This method returns an error in the following cases:
    /// - **Unexpected EOF** (`ErrorKind::UnexpectedEof`): `offset + len > file_size`
    /// - **I/O errors**: Disk read failure, filesystem corruption, device disconnected
    /// - **Invalid offset**: Requesting beyond addressable range (rare on 64-bit systems)
    ///
    /// # Performance
    ///
    /// - **Time complexity**: O(len) for data copying, O(1) for offset calculation
    /// - **Syscalls**: 1 `pread(2)` call
    /// - **Allocations**: 1 heap allocation of `len` bytes
    /// - **Page cache**: Benefits from OS caching; repeated reads of the same range
    ///   may be served from memory without disk I/O
    ///
    /// # Concurrency
    ///
    /// This method is safe to call concurrently from multiple threads. Each call
    /// operates on an independent offset and does not affect other reads.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use hexz_core::store::local::FileBackend;
    /// use hexz_core::store::StorageBackend;
    /// use std::path::Path;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = FileBackend::new(Path::new("/data/snapshot.hxz"))?;
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
        let mut buffer = BytesMut::with_capacity(len);

        // SAFETY: Buffer has been pre-allocated to exactly `len` bytes via with_capacity.
        // read_exact_at will initialize all bytes before we access them, so set_len is safe.
        unsafe {
            buffer.set_len(len);
        }

        match self.inner.read_exact_at(&mut buffer, offset) {
            Ok(_) => Ok(buffer.freeze()),
            Err(e) => Err(hexz_common::Error::Io(e)),
        }
    }

    /// Returns the total file size in bytes.
    ///
    /// This value is cached during construction via `File::metadata()` and remains
    /// constant for the lifetime of the backend. The file is assumed to be immutable
    /// (snapshot semantics); modifying the file externally while the backend is
    /// active results in undefined behavior.
    ///
    /// # Returns
    ///
    /// The file size in bytes as of the time `FileBackend::new()` was called.
    ///
    /// # Performance
    ///
    /// This method is a simple field access with no system calls (O(1)).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use hexz_core::store::local::FileBackend;
    /// use hexz_core::store::StorageBackend;
    /// use std::path::Path;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = FileBackend::new(Path::new("/data/snapshot.hxz"))?;
    /// let size = backend.len();
    /// println!("Snapshot size: {} bytes ({} MB)", size, size / 1024 / 1024);
    /// # Ok(())
    /// # }
    /// ```
    fn len(&self) -> u64 {
        self.size
    }
}
