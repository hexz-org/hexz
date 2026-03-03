//! In-memory index of a parent archive's block hashes for cross-file deduplication.

use std::collections::HashSet;
use std::sync::Arc;

use hexz_common::Result;
use hexz_core::ArchiveStream;
use hexz_core::api::file::Archive;

/// A Bloom-filter-like index of hashes present in a parent archive.
///
/// Used during the `pack` operation to identify blocks that can be stored
/// as `parent_ref` instead of being re-written.
pub struct ParentIndex {
    /// Set of BLAKE3 hashes available in the parent archive(s)
    pub hashes: HashSet<[u8; 32]>,
}

impl ParentIndex {
    /// Creates a new parent index from one or more parent archives.
    pub fn new(parents: &[Arc<Archive>]) -> Result<Self> {
        let mut hashes = HashSet::new();
        for parent in parents {
            for hash in parent.iter_block_hashes(ArchiveStream::Main)? {
                hashes.insert(hash);
            }
        }
        Ok(Self { hashes })
    }
}
