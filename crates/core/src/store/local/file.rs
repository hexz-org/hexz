//! Local File System Storage Backend.
//!
//! This module implements the `StorageBackend` trait for files residing on the
//! local filesystem. It leverages standard POSIX-style file I/O operations
//! to provide reliable and efficient access to snapshot data stored on disk.
//!
//! This backend is optimized for concurrency by using offset-based reads (`pread`),
//! allowing multiple threads to read from the same file handle without lock contention
//! or race conditions on the file cursor.

use crate::store::StorageBackend;
use bytes::{Bytes, BytesMut};
use std::fs::File;
use std::os::unix::fs::FileExt;
use strata_common::Result;

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
    /// Initializes a new local file backend.
    ///
    /// This constructor opens the file at the specified path in read-only mode
    /// and queries its metadata to determine the total size. This size is cached
    /// to avoid repeated `stat` syscalls during operation.
    ///
    /// # Arguments
    ///
    /// * `path` - The filesystem path to the snapshot file.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the initialized `FileBackend` on success,
    /// or an error if the file cannot be opened or its metadata cannot be read.
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
    /// Reads a specific range of bytes from the local file using stateless I/O.
    ///
    /// This method invokes the underlying operating system's `pread` (or equivalent)
    /// functionality to read data from the specified absolute offset. This operation
    /// does not modify the file's current read position, making it safe for concurrent
    /// use by multiple threads.
    ///
    /// # Arguments
    ///
    /// * `offset` - The byte offset from the start of the file.
    /// * `len` - The number of bytes to read.
    ///
    /// # Returns
    ///
    /// Returns a `Bytes` buffer containing the requested data. Returns an error if the
    /// read operation fails or if the end of the file is reached unexpectedly.
    fn read_exact(&self, offset: u64, len: usize) -> Result<Bytes> {
        let mut buffer = BytesMut::with_capacity(len);

        unsafe {
            buffer.set_len(len);
        }

        match self.inner.read_exact_at(&mut buffer, offset) {
            Ok(_) => Ok(buffer.freeze()),
            Err(e) => Err(strata_common::StrataError::Io(e)),
        }
    }

    /// Returns the cached file size in bytes.
    ///
    /// This value is retrieved once during initialization and is assumed to be
    /// constant for the lifetime of the backend instance (immutable snapshot).
    fn len(&self) -> u64 {
        self.size
    }
}
