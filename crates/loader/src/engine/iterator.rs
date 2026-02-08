//! Sequential and threaded iteration over snapshot samples.
//!
//! Provides an iterator abstraction over snapshot data that supports
//! prefetching and multithreaded access patterns for ML data loading.

use std::sync::Arc;
use strata_core::StrataFile;
use strata_core::api::stratafile::SnapshotStream;

/// Configuration for snapshot iteration.
pub struct IterConfig {
    /// Block size for each read operation.
    pub block_size: usize,
    /// Number of blocks to prefetch ahead.
    pub prefetch_count: usize,
    /// Which stream to iterate over.
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

/// Iterator that reads sequential blocks from a snapshot stream.
pub struct SnapshotIterator {
    snap: Arc<StrataFile>,
    config: IterConfig,
    offset: u64,
    total_size: u64,
}

impl SnapshotIterator {
    /// Creates a new iterator over a snapshot stream.
    pub fn new(snap: Arc<StrataFile>, config: IterConfig) -> Self {
        let total_size = snap.size(config.stream);
        Self {
            snap,
            config,
            offset: 0,
            total_size,
        }
    }

    /// Resets the iterator to the beginning.
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
