//! In-memory index of a parent snapshot's block hashes for cross-file deduplication.

use std::collections::HashSet;
use std::sync::Arc;

use crate::api::file::File;
use hexz_common::Result;

/// A set of all unique block hashes from one or more parent snapshots.
///
/// This is used to decide whether to write a new data block or a cheap
/// parent-reference block. Since hashes are stored in the .hxz index,
/// building this is extremely fast and requires no data I/O.
pub struct ParentIndex {
    pub(crate) hashes: HashSet<[u8; 32]>,
}

impl ParentIndex {
    /// Creates a new index by collecting pre-calculated hashes from the indices
    /// of multiple parent files.
    ///
    /// This is an O(N_blocks) operation that is nearly instant as it only
    /// reads metadata, not actual block data.
    pub fn new(parents: &[Arc<File>]) -> Result<Self> {
        let mut hashes = HashSet::new();

        for parent in parents {
            // Collect hashes from the primary stream.
            let pages = &parent.master.primary_pages;
            for page_entry in pages {
                // get_page fetches/deserializes the index page (O(1) if cached).
                let page = parent.get_page(page_entry)?;
                for block_info in &page.blocks {
                    // Skip sparse blocks (all zeros, handled separately).
                    // We collect all valid content hashes, including those from
                    // parent references, so deduplication works across chains.
                    if !block_info.is_sparse() && block_info.hash != [0u8; 32] {
                        hashes.insert(block_info.hash);
                    }
                }
            }
        }

        Ok(Self { hashes })
    }
}
