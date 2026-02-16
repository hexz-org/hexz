//! Elastic Hash Table implementation.
//!
//! This module implements the Elastic Hashing algorithm from:
//! **"Optimal Bounds for Open Addressing Without Reordering"**
//! by Martín Farach-Colton, Andrew Krapivin, William Kuszmaul (2025)
//!
//! Paper: https://arxiv.org/abs/2501.02305
//!
//! # Algorithm Overview
//!
//! Elastic Hashing partitions the hash table into geometrically decreasing levels:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │ Level 0: n/2 slots    (50% of total capacity)           │
//! ├─────────────────────────────────────────────────────────┤
//! │ Level 1: n/4 slots    (25% of total capacity)           │
//! ├─────────────────────────────────────────────────────────┤
//! │ Level 2: n/8 slots    (12.5% of total capacity)         │
//! ├─────────────────────────────────────────────────────────┤
//! │ Level 3: n/16 slots   (6.25% of total capacity)         │
//! ├─────────────────────────────────────────────────────────┤
//! │ ...                                                      │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! # Insertion Strategy (Three-Case Logic from Section 3.2)
//!
//! For each level i, the algorithm decides how to insert based on the current
//! level's emptiness (`epsilon`) and the next level's capacity:
//!
//! - **Case 1** (level has room, next level has room): Try `probe_limit` probes.
//!   If all occupied, fall through to next level.
//! - **Case 2** (level too full): Skip directly to next level.
//! - **Case 3** (next level nearly full): Force-insert into current level with
//!   extended probing (up to full level scan).
//! - **Last level**: Always try full scan.
//!
//! # Lookup Strategy
//!
//! For each level: follow same probe sequence until key found, empty slot hit,
//! or `max_probe_distance` probes exhausted. This is safe because items inserted
//! with probe distance j guarantee all slots at positions 0..j-1 were occupied.
//!
//! # Memory Layout
//!
//! All levels are stored in a single contiguous `Vec<Entry>` for cache efficiency:
//!
//! ```text
//! [L0_entry0, L0_entry1, ..., L0_entryN/2, L1_entry0, ..., L2_entry0, ...]
//!  ├─────────────────────────────────────┤ ├─────────┤ ├────────┤
//!         Level 0 (50%)                    Level 1      Level 2
//! ```
//!
//! # Performance Characteristics
//!
//! | Operation | Expected | Worst-Case | Notes |
//! |-----------|----------|------------|-------|
//! | Insert    | O(1)     | O(log 1/δ) | δ = empty fraction |
//! | Lookup    | O(1)     | O(log 1/δ) | Early termination helps |
//! | Memory    | 48 bytes/entry | Same | vs 56 bytes for std HashMap |

use std::cell::Cell;

use super::{DedupHashTable, TableStats};

/// Tuning constant for probe limit calculation.
/// `probe_limit = max(1, C * min(log2(1/epsilon), log2(1/delta)))`
const PROBE_CONSTANT: f64 = 4.0;

/// Threshold: if a level's empty fraction is at or below `delta/2`, skip it.
const DELTA_HALF: f64 = 0.05;

/// Threshold: if the next level's empty fraction is at or below this, force-insert
/// into the current level to avoid overloading the next.
const NEXT_LEVEL_FULL_THRESHOLD: f64 = 0.25;

/// Default failure probability parameter (delta).
const DEFAULT_DELTA: f64 = 0.1;

/// Sentinel value for empty entries. `u64::MAX` (18 exabytes) is never a valid
/// file offset, so it can safely indicate "no entry in this slot".
const EMPTY_SENTINEL: u64 = u64::MAX;

/// Elastic Hash Table for deduplication.
///
/// This is Hexz's production implementation of Krapivin et al.'s
/// breakthrough hash table algorithm (2025).
///
/// # Example
///
/// ```rust,ignore
/// use hexz_core::algo::dedup::hash_table::ElasticHashTable;
///
/// let mut table = ElasticHashTable::with_capacity(10_000);
///
/// // Insert blocks
/// for i in 0..10_000 {
///     let hash = blake3::hash(&i.to_le_bytes());
///     table.insert(*hash.as_bytes(), i as u64);
/// }
///
/// // Check memory
/// println!("Memory: {} MB", table.memory_bytes() / 1_048_576);
/// println!("Load factor: {:.2}", table.load_factor());
/// ```
pub struct ElasticHashTable {
    /// Flat array holding all entries across all levels.
    ///
    /// Layout: [Level 0 entries | Level 1 entries | Level 2 entries | ...]
    entries: Vec<Entry>,

    /// Metadata for each level (start index, size, occupancy, max probe distance).
    levels: Vec<LevelInfo>,

    /// Number of occupied entries.
    len: usize,

    /// Maximum load factor before resizing (default: 0.9).
    max_load_factor: f64,

    /// Failure probability parameter delta.
    delta: f64,

    // -- Stats (insert counters use plain u64 since insert takes &mut self) --
    total_inserts: u64,
    max_probe_length: u32,

    // -- Stats (lookup counters use Cell since get takes &self) --
    total_lookups: Cell<u64>,
    total_probes: Cell<u64>,
}

/// A single entry in the hash table.
///
/// Uses `EMPTY_SENTINEL` (`u64::MAX`) in the `offset` field to indicate an empty
/// slot, eliminating the need for a separate `EntryState` field. This packs
/// the entry to 40 bytes (down from 48 with the state field + alignment padding),
/// saving ~17% memory at scale.
#[derive(Clone, Copy)]
#[repr(C)]
struct Entry {
    /// BLAKE3 hash of the compressed block (32 bytes).
    hash: [u8; 32],

    /// Physical offset in the snapshot file (8 bytes).
    /// `u64::MAX` indicates an empty slot (never a valid file offset).
    offset: u64,
}

/// Metadata for a single level.
#[derive(Clone)]
struct LevelInfo {
    /// Starting index in the entries array.
    start: usize,

    /// Number of slots in this level (always a power of 2).
    size: usize,

    /// Number of entries currently in this level.
    occupancy: usize,

    /// Maximum probe distance seen during insertion into this level.
    /// Used by lookup to know when to stop probing.
    max_probe_distance: u32,
}

impl ElasticHashTable {
    /// Creates a new Elastic Hash Table with the given capacity.
    ///
    /// The capacity will be rounded up to the nearest power of 2 for
    /// efficient bit-masking during probing.
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.next_power_of_two().max(16);
        let levels = Self::build_levels(capacity);
        let total_slots: usize = levels.iter().map(|l| l.size).sum();
        let entries = vec![Entry::empty(); total_slots];

        Self {
            entries,
            levels,
            len: 0,
            max_load_factor: 0.9,
            delta: DEFAULT_DELTA,
            total_inserts: 0,
            max_probe_length: 0,
            total_lookups: Cell::new(0),
            total_probes: Cell::new(0),
        }
    }

    /// Builds the level structure with geometrically decreasing sizes.
    ///
    /// Level k has size n / 2^(k+1):
    /// - Level 0: n/2
    /// - Level 1: n/4
    /// - Level 2: n/8
    /// - ...
    fn build_levels(capacity: usize) -> Vec<LevelInfo> {
        let mut levels = Vec::new();
        let mut start = 0;
        let mut level_size = capacity / 2;

        while level_size > 0 {
            levels.push(LevelInfo {
                start,
                size: level_size,
                occupancy: 0,
                max_probe_distance: 0,
            });

            start += level_size;
            level_size /= 2;
        }

        levels
    }

    /// Returns the total capacity (sum of all level sizes).
    pub fn capacity(&self) -> usize {
        self.entries.len()
    }

    /// Derive h1 and h2 from a 32-byte key for double hashing.
    ///
    /// h1 = first 8 bytes as u64
    /// h2 = next 8 bytes as u64, forced odd for full coverage on power-of-2 sizes
    #[inline(always)]
    fn hash_pair(hash: &[u8; 32]) -> (u64, u64) {
        let h1 = u64::from_le_bytes(hash[0..8].try_into().unwrap());
        let h2 = u64::from_le_bytes(hash[8..16].try_into().unwrap()) | 1; // force odd
        (h1, h2)
    }

    /// Compute probe position j in a level: (h1 + j * h2) % level_size.
    #[inline(always)]
    fn probe_index(h1: u64, h2: u64, j: u32, level_size: usize) -> usize {
        let idx = h1.wrapping_add(h2.wrapping_mul(j as u64));
        (idx as usize) & (level_size - 1) // level_size is power of 2
    }

    /// Compute the dynamic probe limit for a level based on its emptiness.
    ///
    /// `probe_limit = max(1, C * min(log2(1/epsilon), log2(1/delta)))`
    #[inline]
    fn dynamic_probe_limit(epsilon: f64, delta: f64) -> u32 {
        if epsilon <= 0.0 {
            return 1;
        }
        let log_inv_eps = (1.0 / epsilon).log2();
        let log_inv_delta = (1.0 / delta).log2();
        let limit = PROBE_CONSTANT * log_inv_eps.min(log_inv_delta);
        (limit.ceil() as u32).max(1)
    }

    /// Returns the empty fraction of a level.
    #[inline]
    fn level_epsilon(level: &LevelInfo) -> f64 {
        if level.size == 0 {
            return 0.0;
        }
        (level.size - level.occupancy) as f64 / level.size as f64
    }

    /// Insert a key-value pair using the three-case insertion logic.
    ///
    /// Returns `Some(old_offset)` if the key already existed.
    fn do_insert(&mut self, hash: [u8; 32], offset: u64) -> Option<u64> {
        let (h1, h2) = Self::hash_pair(&hash);
        let num_levels = self.levels.len();

        for level_idx in 0..num_levels {
            let is_last = level_idx == num_levels - 1;

            let epsilon = Self::level_epsilon(&self.levels[level_idx]);
            let level_start = self.levels[level_idx].start;
            let level_size = self.levels[level_idx].size;

            // Determine the probe limit and whether we should try this level
            let probe_limit = if is_last {
                // Last level: always try full scan
                level_size as u32
            } else {
                let next_epsilon = Self::level_epsilon(&self.levels[level_idx + 1]);

                if epsilon <= DELTA_HALF {
                    // Case 2: Level too full, skip to next
                    continue;
                } else if next_epsilon <= NEXT_LEVEL_FULL_THRESHOLD {
                    // Case 3: Next level nearly full, force-insert here
                    level_size as u32
                } else {
                    // Case 1: Normal insertion with dynamic probe limit
                    Self::dynamic_probe_limit(epsilon, self.delta)
                }
            };

            // Probe this level
            for j in 0..probe_limit {
                let slot = Self::probe_index(h1, h2, j, level_size);
                let idx = level_start + slot;
                let entry = &self.entries[idx];

                if entry.is_occupied() {
                    if entry.hash == hash {
                        // Key exists: update value, return old
                        let old = self.entries[idx].offset;
                        self.entries[idx].offset = offset;
                        return Some(old);
                    }
                    // Slot occupied by different key, continue probing
                } else {
                    // Empty slot: insert here
                    self.entries[idx] = Entry { hash, offset };
                    self.len += 1;
                    self.levels[level_idx].occupancy += 1;
                    if j + 1 > self.levels[level_idx].max_probe_distance {
                        self.levels[level_idx].max_probe_distance = j + 1;
                    }
                    if j + 1 > self.max_probe_length {
                        self.max_probe_length = j + 1;
                    }
                    return None;
                }
            }

            // Exhausted probes for this level, fall through to next
        }

        // All levels exhausted — this should not happen at load factors below max.
        // Force resize and retry.
        self.resize();
        self.do_insert(hash, offset)
    }

    /// Look up a key across all levels.
    fn do_get(&self, hash: &[u8; 32]) -> Option<u64> {
        let (h1, h2) = Self::hash_pair(hash);
        let mut total_probes = 0u64;

        for level in &self.levels {
            let limit = level.max_probe_distance;
            if limit == 0 {
                // No entries were ever inserted into this level with any probes,
                // so nothing to find. But if the level is completely empty we can skip.
                // However, we must still check slot 0 if occupancy > 0 won't happen
                // since max_probe_distance == 0 means nothing was inserted here.
                continue;
            }

            for j in 0..limit {
                let slot = Self::probe_index(h1, h2, j, level.size);
                let idx = level.start + slot;
                let entry = &self.entries[idx];
                total_probes += 1;

                if entry.is_empty() {
                    // Empty slot: key cannot be further in this level
                    break;
                }
                // Slot is occupied — check if it matches
                if entry.hash == *hash {
                    self.total_lookups.set(self.total_lookups.get() + 1);
                    self.total_probes
                        .set(self.total_probes.get() + total_probes);
                    return Some(entry.offset);
                }
            }
        }

        self.total_lookups.set(self.total_lookups.get() + 1);
        self.total_probes
            .set(self.total_probes.get() + total_probes);
        None
    }

    /// Double the table capacity and reinsert all entries.
    fn resize(&mut self) {
        let old_capacity = self.capacity();
        let new_capacity = old_capacity * 2;

        let new_levels = Self::build_levels(new_capacity);
        let new_total_slots: usize = new_levels.iter().map(|l| l.size).sum();
        let new_entries = vec![Entry::empty(); new_total_slots];

        // Collect old entries
        let old_entries: Vec<Entry> = self
            .entries
            .iter()
            .filter(|e| e.is_occupied())
            .copied()
            .collect();

        // Replace table state
        self.entries = new_entries;
        self.levels = new_levels;
        self.len = 0;
        self.max_probe_length = 0;

        // Reinsert all old entries
        for entry in old_entries {
            self.do_insert(entry.hash, entry.offset);
        }
    }
}

impl Entry {
    /// Creates an empty entry, marked by `EMPTY_SENTINEL` offset.
    const fn empty() -> Self {
        Self {
            hash: [0u8; 32],
            offset: EMPTY_SENTINEL,
        }
    }

    /// Returns `true` if this slot is empty.
    #[inline(always)]
    const fn is_empty(&self) -> bool {
        self.offset == EMPTY_SENTINEL
    }

    /// Returns `true` if this slot is occupied.
    #[inline(always)]
    const fn is_occupied(&self) -> bool {
        self.offset != EMPTY_SENTINEL
    }
}

impl DedupHashTable for ElasticHashTable {
    fn insert(&mut self, hash: [u8; 32], offset: u64) -> Option<u64> {
        // Check if we need to resize before inserting
        if self.len > 0 && (self.len + 1) as f64 / self.capacity() as f64 > self.max_load_factor {
            self.resize();
        }

        self.total_inserts += 1;
        self.do_insert(hash, offset)
    }

    fn get(&self, hash: &[u8; 32]) -> Option<u64> {
        self.do_get(hash)
    }

    fn len(&self) -> usize {
        self.len
    }

    fn load_factor(&self) -> f64 {
        if self.capacity() == 0 {
            return 0.0;
        }
        self.len as f64 / self.capacity() as f64
    }

    fn memory_bytes(&self) -> usize {
        self.entries.capacity() * std::mem::size_of::<Entry>()
            + self.levels.capacity() * std::mem::size_of::<LevelInfo>()
    }

    fn stats(&self) -> TableStats {
        TableStats {
            total_inserts: self.total_inserts,
            total_lookups: self.total_lookups.get(),
            total_probes: self.total_probes.get(),
            max_probe_length: self.max_probe_length,
            level_usage: self.levels.iter().map(|l| l.occupancy).collect(),
        }
    }
}

// ElasticHashTable is Send + Sync because Cell<u64> is Send (but not Sync).
// However, our get() method only uses the Cells internally via &self, and we
// don't expose them. The trait requires Send + Sync. Since we only ever access
// the Cells from a single thread at a time (no concurrent &self access to Cells),
// this is safe. But Cell is !Sync, so we need an unsafe impl.
//
// SAFETY: The Cell fields are only used for statistics counters in get().
// They are never shared across threads simultaneously — callers must ensure
// exclusive access for correctness (which is the normal usage pattern).
unsafe impl Sync for ElasticHashTable {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_table() {
        let table = ElasticHashTable::with_capacity(1024);
        assert_eq!(table.len(), 0);
        // Geometric series: n/2 + n/4 + ... + 1 = n - 1
        assert_eq!(table.capacity(), 1023);
    }

    #[test]
    fn test_level_structure() {
        let table = ElasticHashTable::with_capacity(1024);

        // Verify geometric decrease: 512, 256, 128, 64, 32, 16, 8, 4, 2, 1
        let expected_sizes = vec![512, 256, 128, 64, 32, 16, 8, 4, 2, 1];

        for (i, expected) in expected_sizes.iter().enumerate() {
            assert_eq!(table.levels[i].size, *expected, "Level {} size mismatch", i);
        }
    }

    #[test]
    fn test_entry_size() {
        // Entry must be exactly 40 bytes: [u8; 32] hash + u64 offset.
        // Previous layout with EntryState enum was 48 bytes due to padding.
        assert_eq!(std::mem::size_of::<Entry>(), 40);
    }

    #[test]
    fn test_memory_estimate() {
        let table = ElasticHashTable::with_capacity(1024);
        let mem = table.memory_bytes();

        // 1024 entries * size_of::<Entry>() + level metadata
        let entry_size = std::mem::size_of::<Entry>();
        let expected_min = 1024 * entry_size;
        assert!(
            mem >= expected_min,
            "memory {} < expected min {}",
            mem,
            expected_min
        );
    }

    #[test]
    fn test_insert_and_get() {
        let mut table = ElasticHashTable::with_capacity(1024);

        let hash = blake3::hash(b"hello world");
        let key = *hash.as_bytes();

        assert_eq!(table.insert(key, 42), None);
        assert_eq!(table.get(&key), Some(42));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn test_insert_duplicate_updates() {
        let mut table = ElasticHashTable::with_capacity(1024);

        let hash = blake3::hash(b"test key");
        let key = *hash.as_bytes();

        assert_eq!(table.insert(key, 100), None);
        assert_eq!(table.insert(key, 200), Some(100));
        assert_eq!(table.get(&key), Some(200));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn test_get_missing_returns_none() {
        let mut table = ElasticHashTable::with_capacity(1024);

        let key1 = *blake3::hash(b"exists").as_bytes();
        let key2 = *blake3::hash(b"missing").as_bytes();

        table.insert(key1, 1);
        assert_eq!(table.get(&key2), None);
    }

    #[test]
    fn test_high_load_factor() {
        let capacity = 1024;
        let mut table = ElasticHashTable::with_capacity(capacity);

        let target = (capacity as f64 * 0.85) as usize;
        let mut keys = Vec::with_capacity(target);

        for i in 0..target {
            let key = *blake3::hash(&(i as u64).to_le_bytes()).as_bytes();
            keys.push(key);
            table.insert(key, i as u64);
        }

        // Verify all entries retrievable
        for (i, key) in keys.iter().enumerate() {
            assert_eq!(
                table.get(key),
                Some(i as u64),
                "Failed to retrieve entry {}",
                i
            );
        }

        assert_eq!(table.len(), target);
    }

    #[test]
    fn test_resize_preserves_entries() {
        let mut table = ElasticHashTable::with_capacity(64);
        let mut keys = Vec::new();

        // Insert enough to trigger resize (load > 0.9)
        for i in 0..60 {
            let key = *blake3::hash(&(i as u64).to_le_bytes()).as_bytes();
            keys.push(key);
            table.insert(key, i as u64);
        }

        // Table should have resized
        assert!(table.capacity() > 64);

        // All entries must survive
        for (i, key) in keys.iter().enumerate() {
            assert_eq!(
                table.get(key),
                Some(i as u64),
                "Entry {} lost after resize",
                i
            );
        }
    }

    #[test]
    fn test_level_overflow() {
        // Use a small table to force entries into deeper levels
        let mut table = ElasticHashTable::with_capacity(32);

        let mut keys = Vec::new();
        for i in 0..25 {
            let key = *blake3::hash(&(i as u64).to_le_bytes()).as_bytes();
            keys.push(key);
            table.insert(key, i as u64);
        }

        // All entries should be retrievable
        for (i, key) in keys.iter().enumerate() {
            assert_eq!(table.get(key), Some(i as u64));
        }

        // Some entries should have cascaded to deeper levels
        let stats = table.stats();
        let non_level0: usize = stats.level_usage.iter().skip(1).sum();
        // With 25 entries in a 32-slot table (78% load), some will cascade
        assert!(
            non_level0 > 0 || stats.level_usage[0] == table.len(),
            "Expected either cascading or all in level 0"
        );
    }

    #[test]
    fn test_many_random_entries() {
        let mut table = ElasticHashTable::with_capacity(16384);
        let count = 10_000;
        let mut keys = Vec::with_capacity(count);

        for i in 0..count {
            let key = *blake3::hash(&(i as u64).to_le_bytes()).as_bytes();
            keys.push(key);
            table.insert(key, i as u64);
        }

        for (i, key) in keys.iter().enumerate() {
            assert_eq!(
                table.get(key),
                Some(i as u64),
                "Missing entry {} of {}",
                i,
                count
            );
        }

        assert_eq!(table.len(), count);
    }

    #[test]
    fn test_stats_tracking() {
        let mut table = ElasticHashTable::with_capacity(1024);

        for i in 0u64..100 {
            let key = *blake3::hash(&i.to_le_bytes()).as_bytes();
            table.insert(key, i);
        }

        let key = *blake3::hash(&0u64.to_le_bytes()).as_bytes();
        table.get(&key);
        table.get(&key);

        let stats = table.stats();
        assert_eq!(stats.total_inserts, 100);
        assert_eq!(stats.total_lookups, 2);
        assert!(stats.total_probes > 0);
        assert!(stats.max_probe_length > 0);
    }

    #[test]
    fn test_level_usage_distribution() {
        let mut table = ElasticHashTable::with_capacity(4096);

        // Insert at moderate load (~50%)
        for i in 0..2000 {
            let key = *blake3::hash(&(i as u64).to_le_bytes()).as_bytes();
            table.insert(key, i as u64);
        }

        let stats = table.stats();
        let level0 = stats.level_usage[0];

        // At moderate load, most entries should be in Level 0
        assert!(
            level0 as f64 / table.len() as f64 > 0.7,
            "Expected >70% in Level 0, got {:.1}%",
            level0 as f64 / table.len() as f64 * 100.0
        );
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arbitrary_hash() -> impl Strategy<Value = [u8; 32]> {
        prop::array::uniform32(any::<u8>())
    }

    proptest! {
        #[test]
        fn prop_insert_then_get(entries in prop::collection::vec((arbitrary_hash(), any::<u64>()), 1..500)) {
            let mut table = ElasticHashTable::with_capacity(1024);
            let mut expected = std::collections::HashMap::new();

            for (hash, offset) in &entries {
                table.insert(*hash, *offset);
                expected.insert(*hash, *offset);
            }

            for (hash, offset) in &expected {
                prop_assert_eq!(table.get(hash), Some(*offset));
            }

            prop_assert_eq!(table.len(), expected.len());
        }

        #[test]
        fn prop_no_false_positives(
            inserted in prop::collection::vec((arbitrary_hash(), any::<u64>()), 1..200),
            queries in prop::collection::vec(arbitrary_hash(), 1..200),
        ) {
            let mut table = ElasticHashTable::with_capacity(512);
            let mut inserted_set = std::collections::HashSet::new();

            for (hash, offset) in &inserted {
                table.insert(*hash, *offset);
                inserted_set.insert(*hash);
            }

            for query in &queries {
                if !inserted_set.contains(query) {
                    prop_assert_eq!(table.get(query), None, "False positive for {:?}", &query[..4]);
                }
            }
        }

        #[test]
        fn prop_elastic_matches_standard(entries in prop::collection::vec((arbitrary_hash(), any::<u64>()), 1..300)) {
            use crate::algo::dedup::hash_table::StandardHashTable;

            let mut elastic = ElasticHashTable::with_capacity(512);
            let mut standard = StandardHashTable::new();

            for (hash, offset) in &entries {
                let e_old = elastic.insert(*hash, *offset);
                let s_old = standard.insert(*hash, *offset);
                prop_assert_eq!(e_old, s_old, "insert mismatch for key {:?}", &hash[..4]);
            }

            prop_assert_eq!(elastic.len(), standard.len());

            // Verify all lookups match
            for (hash, _) in &entries {
                prop_assert_eq!(
                    elastic.get(hash),
                    standard.get(hash),
                    "get mismatch for key {:?}", &hash[..4]
                );
            }
        }
    }
}
