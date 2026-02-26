//! Memory-mapped file storage backend.
//!
//! This is the **baseline** implementation used by the paper's benchmarks.
//! All novel mmap strategies (arena reset, huge pages, prefault, io_uring)
//! are implemented alongside this module for direct comparison.

use bytes::Bytes;
use hexz_common::Result;
use hexz_core::store::StorageBackend;
use memmap2::Mmap;
use std::fs::File;

/// A read-only memory-mapped file backend.
///
/// Maps the entire file into the process address space with `MAP_PRIVATE | PROT_READ`.
/// Page faults are handled transparently by the kernel — this is the naive baseline
/// that the paper's novel strategies aim to beat.
///
/// Reads are zero-copy: `read_exact` returns a `Bytes` slice backed by the mmap
/// region, avoiding `memcpy` entirely.
#[derive(Debug)]
pub struct MmapBackend {
    bytes: Bytes,
    len: u64,
}

impl MmapBackend {
    /// Opens and maps `path` read-only. No I/O occurs until pages are accessed.
    pub fn new(path: &std::path::Path) -> Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        // SAFETY: The file is immutable for the lifetime of the mapping (snapshot semantics).
        let map = unsafe { Mmap::map(&file)? };
        // Wrap the Mmap in Bytes via from_owner so that slicing is zero-copy.
        // The Mmap is moved into the Bytes and kept alive by its refcount.
        let bytes = Bytes::from_owner(map);
        Ok(Self { bytes, len })
    }
}

impl StorageBackend for MmapBackend {
    fn read_exact(&self, offset: u64, len: usize) -> Result<Bytes> {
        let start = offset as usize;
        let end = start + len;
        if end > self.bytes.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Read out of bounds",
            )
            .into());
        }
        Ok(self.bytes.slice(start..end))
    }

    fn len(&self) -> u64 {
        self.len
    }
}
