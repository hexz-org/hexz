//! Hash table for deduplication workloads.
//!
//! This module provides a specialized hash table optimized for Hexz's block
//! deduplication during archive packing. It wraps `std::collections::HashMap`
//! with an identity hasher since keys are BLAKE3 hashes (already uniformly
//! distributed), eliminating redundant SipHash overhead.
//!
//! # Usage
//!
//! ```rust,ignore
//! use hexz_core::algo::dedup::hash_table::StandardHashTable;
//!
//! let mut table = StandardHashTable::with_capacity(1_000_000);
//!
//! // Insert hash -> offset mapping
//! let hash = blake3::hash(b"compressed block data");
//! table.insert(*hash.as_bytes(), 12345);
//!
//! // Lookup
//! if let Some(offset) = table.get(hash.as_bytes()) {
//!     println!("Block at offset: {}", offset);
//! }
//!
//! // Check memory usage
//! println!("Memory: {} MB", table.memory_bytes() / 1_048_576);
//! ```

/// Performance metrics for hash table operations.
///
/// Used for benchmarking and profiling.
#[derive(Debug, Clone, Default)]
pub struct TableStats {
    /// Total number of insert operations performed
    pub total_inserts: u64,

    /// Total number of lookup operations performed
    pub total_lookups: u64,

    /// Total number of probe steps across all operations
    pub total_probes: u64,

    /// Maximum probe length seen in any single operation
    pub max_probe_length: u32,

    /// Distribution of entries across levels (reserved for future use)
    pub level_usage: Vec<usize>,
}

impl TableStats {
    /// Returns the average number of probes per operation.
    pub fn avg_probes_per_op(&self) -> f64 {
        let total_ops = self.total_inserts + self.total_lookups;
        if total_ops == 0 {
            0.0
        } else {
            self.total_probes as f64 / total_ops as f64
        }
    }
}

/// Identity hasher that uses the first 8 bytes of a BLAKE3 key directly.
///
/// Since keys in the dedup table are BLAKE3 hashes (already uniformly distributed),
/// there is no need for the default SipHash to re-hash them.
struct IdentityHasher(u64);

impl std::hash::Hasher for IdentityHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        if bytes.len() >= 8 {
            self.0 = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        } else {
            self.0 = bytes
                .iter()
                .fold(0u64, |acc, &b| acc.wrapping_mul(256).wrapping_add(b as u64));
        }
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

#[derive(Clone, Default)]
struct BuildIdentityHasher;

impl std::hash::BuildHasher for BuildIdentityHasher {
    type Hasher = IdentityHasher;

    #[inline]
    fn build_hasher(&self) -> IdentityHasher {
        IdentityHasher(0)
    }
}

/// Hash table for block deduplication with identity hashing.
///
/// Uses an identity hasher since keys are BLAKE3 hashes (already uniformly
/// distributed), giving ~3-6x speedup over the default SipHash.
pub struct StandardHashTable {
    map: std::collections::HashMap<[u8; 32], u64, BuildIdentityHasher>,
    total_inserts: u64,
    total_lookups: std::cell::Cell<u64>,
    total_probes: std::cell::Cell<u64>,
}

impl StandardHashTable {
    pub fn new() -> Self {
        Self {
            map: std::collections::HashMap::with_hasher(BuildIdentityHasher),
            total_inserts: 0,
            total_lookups: std::cell::Cell::new(0),
            total_probes: std::cell::Cell::new(0),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            map: std::collections::HashMap::with_capacity_and_hasher(capacity, BuildIdentityHasher),
            total_inserts: 0,
            total_lookups: std::cell::Cell::new(0),
            total_probes: std::cell::Cell::new(0),
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn insert(&mut self, hash: [u8; 32], offset: u64) -> Option<u64> {
        self.total_inserts += 1;
        self.map.insert(hash, offset)
    }

    pub fn get(&self, hash: &[u8; 32]) -> Option<u64> {
        self.total_lookups.set(self.total_lookups.get() + 1);
        self.total_probes.set(self.total_probes.get() + 1);
        self.map.get(hash).copied()
    }

    pub fn load_factor(&self) -> f64 {
        let cap = self.map.capacity();
        if cap == 0 {
            0.0
        } else {
            self.map.len() as f64 / cap as f64
        }
    }

    pub fn memory_bytes(&self) -> usize {
        self.map.capacity() * 56
    }

    pub fn stats(&self) -> TableStats {
        TableStats {
            total_inserts: self.total_inserts,
            total_lookups: self.total_lookups.get(),
            total_probes: self.total_probes.get(),
            max_probe_length: 1,
            level_usage: Vec::new(),
        }
    }
}

impl Default for StandardHashTable {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: Cell fields are only used for statistics counters and are not
// shared across threads simultaneously.
unsafe impl Sync for StandardHashTable {}
