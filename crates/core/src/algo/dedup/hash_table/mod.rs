//! High-performance hash tables for deduplication workloads.
//!
//! This module provides specialized hash table implementations optimized for
//! Hexz's block deduplication during snapshot packing. It includes:
//!
//! - **Trait abstraction** (`DedupHashTable`) for swappable implementations
//! - **Standard wrapper** around `std::collections::HashMap` as baseline
//! - **Elastic Hash Table** implementing Krapivin et al.'s 2025 breakthrough
//!
//! # Background: The Krapivin Breakthrough (2025)
//!
//! In January 2025, Andrew Krapivin (then an undergraduate at Rutgers) published
//! a paper with Farach-Colton and Kuszmaul that disproved a 40-year-old conjecture
//! by Andrew Yao about the optimality of uniform hashing in open-addressed hash tables.
//!
//! **Paper:** [Optimal Bounds for Open Addressing Without Reordering](https://arxiv.org/abs/2501.02305)
//!
//! **Key contributions:**
//! - Achieves O(1) amortized and O(log 1/δ) worst-case expected probe complexity
//! - Enables load factors >0.9 with good performance (15-20% memory savings)
//! - No element reordering needed (unlike cuckoo hashing)
//! - Disproves Yao's conjecture that uniform probing is optimal
//!
//! # Why This Matters for Hexz
//!
//! Deduplication maps can consume gigabytes of RAM when packing large datasets:
//! - **1M unique blocks** = ~56 MB with std HashMap (at 0.875 load factor)
//! - **1M unique blocks** = ~48 MB with Elastic Hash (at 0.9 load factor)
//! - **10M unique blocks** = 560 MB vs 480 MB (80 MB saved!)
//!
//! Additionally, Elastic Hashing provides better worst-case performance when
//! tables are nearly full, preventing slowdowns during large pack operations.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     DedupHashTable Trait                     │
//! │  (insert, get, len, load_factor, memory_bytes, stats)        │
//! └───────────────────┬─────────────────────────────────────────┘
//!                     │
//!          ┌──────────┴──────────┐
//!          │                     │
//!   ┌──────▼──────┐      ┌──────▼──────────┐
//!   │  Standard   │      │     Elastic      │
//!   │  HashMap    │      │   Hash Table     │
//!   │  (baseline) │      │ (Krapivin 2025)  │
//!   └─────────────┘      └──────────────────┘
//! ```
//!
//! # Usage
//!
//! The `DedupHashTable` trait allows swapping implementations via feature flags:
//!
//! ```rust,ignore
//! use hexz_core::algo::dedup::hash_table::{DedupHashTable, ElasticHashTable};
//!
//! let mut table = ElasticHashTable::with_capacity(1_000_000);
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

// Note: Result will be used when implementing error handling in insert/get

/// High-performance hash table for block deduplication.
///
/// This trait abstracts over different hash table implementations, allowing
/// Hexz to compare and swap between standard HashMap and cutting-edge algorithms
/// like Elastic Hashing without changing call sites.
///
/// # Design Constraints
///
/// This trait is specialized for Hexz's deduplication use case:
/// - **Fixed key type**: `[u8; 32]` (BLAKE3 hash)
/// - **Fixed value type**: `u64` (physical offset in snapshot)
/// - **Insert-only**: No `remove()` — dedup is write-once during packing
/// - **No iteration**: We never enumerate entries
///
/// These constraints allow implementations to optimize for the specific workload.
pub trait DedupHashTable: Send + Sync {
    /// Inserts a hash->offset mapping.
    ///
    /// If the hash already exists, updates the offset and returns the old value.
    /// Otherwise, returns `None`.
    ///
    /// # Performance
    ///
    /// Expected: O(1) amortized
    /// Worst-case: Implementation-dependent
    fn insert(&mut self, hash: [u8; 32], offset: u64) -> Option<u64>;

    /// Looks up a hash and returns the associated offset.
    ///
    /// Returns `None` if the hash is not in the table.
    ///
    /// # Performance
    ///
    /// Expected: O(1)
    /// Worst-case: Implementation-dependent
    fn get(&self, hash: &[u8; 32]) -> Option<u64>;

    /// Returns the number of entries in the table.
    fn len(&self) -> usize;

    /// Returns true if the table is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the current load factor (entries / capacity).
    ///
    /// Load factor indicates how full the table is:
    /// - 0.0 = completely empty
    /// - 0.5 = half full
    /// - 0.9 = 90% full (high load)
    /// - 1.0 = completely full (should never happen)
    fn load_factor(&self) -> f64;

    /// Returns the total memory usage in bytes.
    ///
    /// Includes:
    /// - Entry storage (keys + values)
    /// - Metadata (state flags, level info, etc.)
    /// - Overhead (Vec capacity, allocator metadata)
    fn memory_bytes(&self) -> usize;

    /// Returns performance statistics for analysis and tuning.
    fn stats(&self) -> TableStats;
}

/// Performance metrics for hash table operations.
///
/// Used for benchmarking, profiling, and comparing implementations.
#[derive(Debug, Clone, Default)]
pub struct TableStats {
    /// Total number of insert operations performed
    pub total_inserts: u64,

    /// Total number of lookup operations performed
    pub total_lookups: u64,

    /// Total number of probe steps across all operations
    ///
    /// A probe is a single slot check during insertion or lookup.
    /// Lower is better — indicates fewer collisions.
    pub total_probes: u64,

    /// Maximum probe length seen in any single operation
    ///
    /// Indicates worst-case behavior. Should be kept low (<10).
    pub max_probe_length: u32,

    /// Distribution of entries across levels (Elastic Hash only)
    ///
    /// - Index 0 = Level 0 (largest, 50% of capacity)
    /// - Index 1 = Level 1 (25% of capacity)
    /// - Index 2 = Level 2 (12.5% of capacity)
    /// - ...
    ///
    /// Empty for non-leveled implementations like StandardHashTable.
    pub level_usage: Vec<usize>,
}

impl TableStats {
    /// Returns the average number of probes per operation.
    ///
    /// Ideally close to 1.0 (one probe per operation).
    /// Higher values indicate more collisions.
    pub fn avg_probes_per_op(&self) -> f64 {
        let total_ops = self.total_inserts + self.total_lookups;
        if total_ops == 0 {
            0.0
        } else {
            self.total_probes as f64 / total_ops as f64
        }
    }
}

// Submodules
pub mod elastic;

// Re-exports for convenience
pub use elastic::ElasticHashTable;

/// Blanket `DedupHashTable` impl for `HashMap<[u8; 32], u64>` so that
/// `write_block` can accept either HashMap or ElasticHashTable via `dyn DedupHashTable`.
impl DedupHashTable for std::collections::HashMap<[u8; 32], u64> {
    fn insert(&mut self, hash: [u8; 32], offset: u64) -> Option<u64> {
        std::collections::HashMap::insert(self, hash, offset)
    }

    fn get(&self, hash: &[u8; 32]) -> Option<u64> {
        std::collections::HashMap::get(self, hash).copied()
    }

    fn len(&self) -> usize {
        std::collections::HashMap::len(self)
    }

    fn load_factor(&self) -> f64 {
        let cap = std::collections::HashMap::capacity(self);
        if cap == 0 {
            0.0
        } else {
            std::collections::HashMap::len(self) as f64 / cap as f64
        }
    }

    fn memory_bytes(&self) -> usize {
        std::collections::HashMap::capacity(self) * 56
    }

    fn stats(&self) -> TableStats {
        TableStats::default()
    }
}

/// Baseline hash table wrapping `std::collections::HashMap`.
///
/// Used for comparison benchmarks against [`ElasticHashTable`].
pub struct StandardHashTable {
    map: std::collections::HashMap<[u8; 32], u64>,
    total_inserts: u64,
    total_lookups: std::cell::Cell<u64>,
    total_probes: std::cell::Cell<u64>,
}

impl StandardHashTable {
    pub fn new() -> Self {
        Self {
            map: std::collections::HashMap::new(),
            total_inserts: 0,
            total_lookups: std::cell::Cell::new(0),
            total_probes: std::cell::Cell::new(0),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            map: std::collections::HashMap::with_capacity(capacity),
            total_inserts: 0,
            total_lookups: std::cell::Cell::new(0),
            total_probes: std::cell::Cell::new(0),
        }
    }
}

impl Default for StandardHashTable {
    fn default() -> Self {
        Self::new()
    }
}

impl DedupHashTable for StandardHashTable {
    fn insert(&mut self, hash: [u8; 32], offset: u64) -> Option<u64> {
        self.total_inserts += 1;
        self.map.insert(hash, offset)
    }

    fn get(&self, hash: &[u8; 32]) -> Option<u64> {
        self.total_lookups.set(self.total_lookups.get() + 1);
        self.total_probes.set(self.total_probes.get() + 1);
        self.map.get(hash).copied()
    }

    fn len(&self) -> usize {
        self.map.len()
    }

    fn load_factor(&self) -> f64 {
        let cap = self.map.capacity();
        if cap == 0 {
            0.0
        } else {
            self.map.len() as f64 / cap as f64
        }
    }

    fn memory_bytes(&self) -> usize {
        // Approximate: each entry is key (32) + value (8) + hash (8) + pointer (8) = 56 bytes
        self.map.capacity() * 56
    }

    fn stats(&self) -> TableStats {
        TableStats {
            total_inserts: self.total_inserts,
            total_lookups: self.total_lookups.get(),
            total_probes: self.total_probes.get(),
            max_probe_length: 1, // HashMap doesn't expose this
            level_usage: Vec::new(),
        }
    }
}

// SAFETY: Same justification as ElasticHashTable — Cell fields are only
// used for statistics counters and are not shared across threads simultaneously.
unsafe impl Sync for StandardHashTable {}
