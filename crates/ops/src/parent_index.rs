//! In-memory index of a parent snapshot's block hashes for cross-file deduplication.

use std::collections::HashSet;
use std::sync::Arc;

use hexz_common::Result;
use hexz_core::SnapshotStream;
use hexz_core::api::file::File;

/// A set of all unique block hashes from one or more parent snapshots.
///
/// Used to decide whether to write a new data block or a cheap parent-reference
/// block. Since hashes are stored in the .hxz index, building this is fast and
/// requires no data I/O.
pub struct ParentIndex {
    pub(crate) hashes: HashSet<[u8; 32]>,
}

impl ParentIndex {
    /// Creates a new index by collecting pre-calculated hashes from the indices
    /// of multiple parent files.
    ///
    /// This is an O(N_blocks) operation that reads only metadata, not block data.
    pub fn new(parents: &[Arc<File>]) -> Result<Self> {
        let mut hashes = HashSet::new();
        for parent in parents {
            for hash in parent.iter_block_hashes(SnapshotStream::Primary)? {
                hashes.insert(hash);
            }
        }
        Ok(Self { hashes })
    }
}
