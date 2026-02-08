//! Hash-based index implementation for fast random access.
//!
//! This module provides a hash-map-based index structure that enables O(1)
//! average-case lookup of blocks by ID or content hash. The hash index is
//! particularly efficient for random access patterns and deduplication.
//!
//! **Status:** Stub implementation for future development.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::BlockInfo;

/// Hash-based index for block lookup.
///
/// Uses a hash map to provide constant-time average-case lookup of block
/// metadata by block ID or content hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashIndex {
    /// Mapping from block ID to block information.
    pub entries: HashMap<u64, BlockInfo>,
    /// Optional secondary index mapping content hashes to block IDs.
    /// This enables content-addressable storage and deduplication.
    pub content_hashes: HashMap<[u8; 32], u64>,
}

impl HashIndex {
    /// Creates a new empty hash index.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            content_hashes: HashMap::new(),
        }
    }

    /// Looks up block information by block ID.
    pub fn lookup(&self, block_id: u64) -> Option<&BlockInfo> {
        self.entries.get(&block_id)
    }

    /// Looks up a block ID by its content hash.
    ///
    /// Returns the block ID if a block with the given hash exists,
    /// enabling deduplication of identical blocks.
    pub fn lookup_by_hash(&self, hash: &[u8; 32]) -> Option<u64> {
        self.content_hashes.get(hash).copied()
    }

    /// Inserts a new block into the index.
    pub fn insert(&mut self, block_id: u64, info: BlockInfo) {
        self.entries.insert(block_id, info);
    }

    /// Inserts a content hash mapping for deduplication.
    pub fn insert_hash(&mut self, hash: [u8; 32], block_id: u64) {
        self.content_hashes.insert(hash, block_id);
    }

    /// Returns the total number of indexed blocks.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for HashIndex {
    fn default() -> Self {
        Self::new()
    }
}
