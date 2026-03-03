//! Memory-mapped file storage backend with zero-copy reads.
//!
//! `MmapBackend` maps the entire file into the process address space and returns
//! `Bytes` slices backed by the mapping. Each `read_exact` call is O(1) — just
//! an atomic refcount increment and pointer arithmetic, with no `memcpy`.

use bytes::Bytes;
use hexz_common::Result;
use hexz_core::store::StorageBackend;
use memmap2::Mmap;
use std::fs::File;

/// A read-only memory-mapped file backend with zero-copy reads.
///
/// Maps the entire file into the process address space with `MAP_PRIVATE | PROT_READ`.
/// Reads return `Bytes` slices that reference the mapped region directly, avoiding
/// any allocation or copy on the read path.
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
        // SAFETY: The file is immutable for the lifetime of the mapping (archive semantics).
        let map = unsafe { Mmap::map(&file)? };
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
