//! Memory Mapped Storage Backend.
//!
//! This module implements the `StorageBackend` trait by mapping the entire
//! snapshot file into the process's virtual address space. This leverages the
//! operating system's virtual memory manager for caching, paging, and prefetching,
//! often yielding superior performance for random read workloads on local files.

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
    /// Opens a file and creates a read-only memory map of its contents.
    ///
    /// # Safety
    ///
    /// This function utilizes `Mmap::map`, which is inherently unsafe. Undefined
    /// behavior may occur if the underlying file is modified or truncated by another
    /// process while it is mapped. This backend assumes that the snapshot file
    /// is immutable and will not be modified during the lifetime of the mapping.
    ///
    /// # Arguments
    ///
    /// * `path` - The filesystem path to the snapshot file.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the initialized `MmapBackend` on success,
    /// or an error if the file cannot be opened or mapped.
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
    /// Copies a byte range directly from the memory map into a buffer.
    ///
    /// This operation performs a memory-to-memory copy from the mapped region.
    /// If the requested pages are not currently in RAM, the CPU will trigger a
    /// page fault, and the OS kernel will transparently load the data from disk.
    ///
    /// # Arguments
    ///
    /// * `offset` - The byte offset from the start of the memory map.
    /// * `len` - The number of bytes to copy.
    ///
    /// # Returns
    ///
    /// Returns a `Bytes` buffer containing the copied data. Returns an error if the
    /// requested range exceeds the bounds of the memory map.
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

    /// Returns the size of the mapped file in bytes.
    ///
    /// This value reflects the size of the file at the time it was mapped.
    fn len(&self) -> u64 {
        self.len
    }
}
