# Krapivin Hash Table Implementation Plan

> **Version:** 0.3.0
> **Paper:** [Optimal Bounds for Open Addressing Without Reordering](https://arxiv.org/abs/2501.02305)
> **Authors:** Martín Farach-Colton, Andrew Krapivin, William Kuszmaul (2025)
> **Goal:** First production implementation of the breakthrough hash table in a real-world system

---

## Executive Summary

This document outlines the implementation strategy for integrating Krapivin et al.'s revolutionary hash table into Hexz's deduplication system. This is cutting-edge computer science (published Jan 2025) and Hexz will be the **first production system** to ship this technology.

### Why This Matters

**Current problem:**
- Deduplication map `HashMap<[u8; 32], u64>` can consume GBs of RAM for large datasets
- Standard Rust HashMap degrades at high load factors (>0.875)
- Issue [#116](https://github.com/hexz-org/hexz/issues/116) tracks memory pressure during large packs

**Krapivin's solution:**
- **15-20% memory savings** via >0.9 load factors
- **O(log² 1/δ) worst-case lookups** vs O(1/δ) for standard probing (where δ = empty slot fraction)
- **No reordering overhead** during insertions
- **Disproves Yao's 40-year conjecture** that uniform hashing is optimal

---

## Phase 1: Research & Validation (Week 1)

### Goals
- Deeply understand the paper's algorithms
- Validate theoretical claims with toy implementations
- Identify Rust-specific challenges

### Tasks

#### 1.1 Paper Deep Dive (2 days)
- [ ] Read full paper (40 pages) with focus on Sections 3-5
- [ ] Understand **Elastic Hashing** (Section 3.2) — our primary target
- [ ] Understand **Funnel Hashing** (Section 4) — backup approach if Elastic is too complex
- [ ] Map theoretical bounds to practical constants
- [ ] Document key insights in `docs/explanation/krapivin-hash-table.md`

**Key sections to master:**
- **Section 3.2**: Elastic Hashing construction (geometrically decreasing levels)
- **Theorem 3.3**: O(1) amortized, O(log 1/δ) worst-case bounds
- **Section 5**: Practical implementation considerations
- **Algorithm 1**: Elastic Insert pseudocode (page 12)
- **Algorithm 2**: Elastic Search pseudocode (page 13)

#### 1.2 Reference Implementation Analysis (2 days)
- [ ] Clone and study Roy van Rijn's Java implementation: https://github.com/royvanrijn/optimalopen
- [ ] Clone and study Python implementation: https://github.com/sternma/optopenhash
- [ ] Identify bugs, edge cases, and performance pitfalls in existing code
- [ ] Document lessons learned in `KRAPIVIN_LESSONS.md`

**Key questions to answer:**
- How do they handle resizing?
- What are the actual load factor thresholds?
- How is the level structure laid out in memory?
- What's the performance on real-world data?

#### 1.3 Prototype in Python (1 day)
- [ ] Build minimal Elastic Hash Table in Python (~200 lines)
- [ ] Test with random insertions and lookups
- [ ] Validate O(log² n) worst-case behavior empirically
- [ ] Compare memory usage vs Python's dict at 0.9 load factor

**Deliverable:** `scripts/krapivin_prototype.py` with basic correctness tests

#### 1.4 Design Document (1 day)
- [ ] Write detailed Rust API design
- [ ] Choose between Elastic vs Funnel (recommendation: Elastic for simplicity)
- [ ] Define memory layout and level structure
- [ ] Identify unsafe code boundaries (if any)
- [ ] Plan migration strategy from std::HashMap

**Deliverable:** `docs/adr/0006-krapivin-hash-table.md` (Architecture Decision Record)

---

## Phase 2: Core Implementation (Weeks 2-3)

### Project Structure

Create new module for clean isolation and testing:

```
crates/core/src/
├── algo/
│   ├── dedup/
│   │   ├── mod.rs
│   │   ├── cdc.rs
│   │   ├── dcam.rs
│   │   └── hash_table/          # NEW
│   │       ├── mod.rs            # Public API + trait definition
│   │       ├── elastic.rs        # ElasticHashTable implementation
│   │       ├── levels.rs         # Level structure management
│   │       ├── probing.rs        # Probe sequence generation
│   │       └── stats.rs          # Performance metrics
```

### 2.1 Trait Definition (Day 1)

Define a clean abstraction so we can swap implementations:

```rust
// crates/core/src/algo/dedup/hash_table/mod.rs

/// High-performance hash table optimized for deduplication workloads.
///
/// This trait abstracts over different hash table implementations, allowing
/// us to compare standard HashMap vs Krapivin's Elastic Hashing.
pub trait DedupHashTable {
    /// Inserts a hash->offset mapping. Returns the previous value if key existed.
    fn insert(&mut self, hash: [u8; 32], offset: u64) -> Option<u64>;

    /// Looks up a hash and returns the offset if found.
    fn get(&self, hash: &[u8; 32]) -> Option<u64>;

    /// Returns the number of entries in the table.
    fn len(&self) -> usize;

    /// Returns the current load factor (entries / capacity).
    fn load_factor(&self) -> f64;

    /// Returns memory usage in bytes.
    fn memory_bytes(&self) -> usize;

    /// Returns performance statistics for benchmarking.
    fn stats(&self) -> TableStats;
}

/// Performance metrics for hash table operations.
#[derive(Debug, Clone)]
pub struct TableStats {
    pub total_inserts: u64,
    pub total_lookups: u64,
    pub total_probes: u64,        // Total probe steps across all ops
    pub max_probe_length: u32,    // Longest probe sequence seen
    pub level_usage: Vec<usize>,  // Entries per level (Elastic only)
}
```

**Implementation choices:**
1. `[u8; 32]` key type is **hardcoded** (no generics) for simplicity
2. `u64` value type is **hardcoded** (our offset is always u64)
3. No `remove()` — deduplication is insert-only during packing
4. No iteration — we never need to enumerate entries

### 2.2 Standard HashMap Wrapper (Day 1)

Implement the trait for std::HashMap as baseline:

```rust
// crates/core/src/algo/dedup/hash_table/mod.rs

use std::collections::HashMap;

pub struct StandardHashTable {
    inner: HashMap<[u8; 32], u64>,
    stats: TableStats,
}

impl DedupHashTable for StandardHashTable {
    fn insert(&mut self, hash: [u8; 32], offset: u64) -> Option<u64> {
        self.stats.total_inserts += 1;
        self.inner.insert(hash, offset)
    }

    fn get(&self, hash: &[u8; 32]) -> Option<u64> {
        self.stats.total_lookups += 1;
        self.inner.get(hash).copied()
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn load_factor(&self) -> f64 {
        self.len() as f64 / self.inner.capacity() as f64
    }

    fn memory_bytes(&self) -> usize {
        // Entry size: 32 (key) + 8 (value) + ~8 (overhead) = 48 bytes
        self.inner.capacity() * 48
    }

    fn stats(&self) -> TableStats {
        self.stats.clone()
    }
}
```

**Why this matters:**
- Gives us a performance baseline
- Allows A/B testing in production
- Ensures we don't break existing functionality

### 2.3 Elastic Hash Table — Core Structure (Days 2-3)

Implement the memory layout and level structure:

```rust
// crates/core/src/algo/dedup/hash_table/elastic.rs

use super::levels::LevelArray;

/// Elastic Hash Table using Krapivin et al.'s algorithm.
///
/// The table is partitioned into geometrically decreasing levels:
/// - Level 0: size n/2 (50% of total capacity)
/// - Level 1: size n/4 (25% of total capacity)
/// - Level 2: size n/8 (12.5% of total capacity)
/// - ...
/// - Level k: size n/2^(k+1)
///
/// Insertions try each level in order until success.
pub struct ElasticHashTable {
    /// Flat array holding all entries across all levels
    entries: Vec<Entry>,

    /// Level metadata (start index, size, probe limit)
    levels: Vec<LevelInfo>,

    /// Total number of occupied entries
    len: usize,

    /// Target load factor before resizing (0.9)
    max_load_factor: f64,

    /// Performance tracking
    stats: TableStats,
}

/// A single entry in the hash table.
#[derive(Clone, Copy)]
struct Entry {
    /// BLAKE3 hash of the compressed block
    hash: [u8; 32],

    /// Physical offset in the snapshot file
    offset: u64,

    /// Entry state
    state: EntryState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EntryState {
    Empty,
    Occupied,
}

/// Metadata for a single level in the Elastic structure.
struct LevelInfo {
    /// Starting index in the entries array
    start: usize,

    /// Number of slots in this level
    size: usize,

    /// Maximum probe attempts before giving up (log log n)
    probe_limit: u32,
}

impl ElasticHashTable {
    /// Creates a new Elastic Hash Table with the given capacity.
    ///
    /// The capacity will be rounded up to the nearest power of 2 for
    /// efficient bit-masking during probing.
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.next_power_of_two();

        // Build level structure
        let levels = Self::build_levels(capacity);

        // Allocate flat array for all entries
        let total_slots: usize = levels.iter().map(|l| l.size).sum();
        let entries = vec![Entry::empty(); total_slots];

        Self {
            entries,
            levels,
            len: 0,
            max_load_factor: 0.9,
            stats: TableStats::default(),
        }
    }

    /// Builds the level structure with geometrically decreasing sizes.
    fn build_levels(capacity: usize) -> Vec<LevelInfo> {
        let mut levels = Vec::new();
        let mut start = 0;
        let mut level_size = capacity / 2;
        let mut level_idx = 0;

        while level_size > 0 {
            let probe_limit = (capacity as f64).log2().log2().ceil() as u32;

            levels.push(LevelInfo {
                start,
                size: level_size,
                probe_limit,
            });

            start += level_size;
            level_size /= 2;
            level_idx += 1;
        }

        levels
    }
}

impl Entry {
    fn empty() -> Self {
        Self {
            hash: [0u8; 32],
            offset: 0,
            state: EntryState::Empty,
        }
    }

    fn is_empty(&self) -> bool {
        self.state == EntryState::Empty
    }
}
```

**Key design decisions:**
1. **Single flat array** for all levels — better cache locality than separate allocations
2. **No tombstones** — dedup is insert-only, no deletions needed
3. **Power-of-2 capacity** — enables fast modulo via bit masking
4. **Probe limit = log log n** — matches paper's theoretical bound

### 2.4 Probe Sequence Generation (Day 4)

Implement the probing strategy:

```rust
// crates/core/src/algo/dedup/hash_table/probing.rs

use blake3::Hasher;

/// Generates probe sequences for a given hash.
///
/// Uses double hashing: probe_i = (h1 + i * h2) mod m
/// where h1 and h2 are derived from the BLAKE3 hash.
pub struct ProbeSequence {
    h1: usize,
    h2: usize,
    mask: usize,
    iteration: usize,
}

impl ProbeSequence {
    /// Creates a new probe sequence for the given hash and level size.
    pub fn new(hash: &[u8; 32], level_size: usize) -> Self {
        assert!(level_size.is_power_of_two(), "level_size must be power of 2");

        // Split hash into two parts for double hashing
        let h1 = u64::from_le_bytes(hash[0..8].try_into().unwrap()) as usize;
        let h2 = u64::from_le_bytes(hash[8..16].try_into().unwrap()) as usize;

        // Ensure h2 is odd for full coverage (coprime with power-of-2)
        let h2 = h2 | 1;

        Self {
            h1,
            h2,
            mask: level_size - 1,
            iteration: 0,
        }
    }

    /// Returns the next probe index.
    pub fn next(&mut self) -> usize {
        let index = (self.h1 + self.iteration * self.h2) & self.mask;
        self.iteration += 1;
        index
    }
}
```

**Why double hashing:**
- Better distribution than linear probing
- Full table coverage when h2 is odd
- Fast: just addition and bit masking

### 2.5 Insert Operation (Days 5-6)

Implement the core insertion logic:

```rust
// crates/core/src/algo/dedup/hash_table/elastic.rs

impl ElasticHashTable {
    /// Inserts a hash->offset mapping.
    ///
    /// Returns Some(old_offset) if the hash already existed.
    pub fn insert(&mut self, hash: [u8; 32], offset: u64) -> Option<u64> {
        self.stats.total_inserts += 1;

        // Check if resize needed
        if self.load_factor() > self.max_load_factor {
            self.resize();
        }

        // Try each level in order
        for (level_idx, level) in self.levels.iter().enumerate() {
            match self.try_insert_level(hash, offset, level, level_idx) {
                InsertResult::Inserted => {
                    self.len += 1;
                    self.stats.level_usage[level_idx] += 1;
                    return None;
                }
                InsertResult::Updated(old) => {
                    return Some(old);
                }
                InsertResult::LevelFull => {
                    // Try next level
                    continue;
                }
            }
        }

        // All levels full — should be impossible if resize logic is correct
        panic!("ElasticHashTable: all levels full (bug in resize logic)");
    }

    /// Attempts to insert into a specific level.
    fn try_insert_level(
        &mut self,
        hash: [u8; 32],
        offset: u64,
        level: &LevelInfo,
        level_idx: usize,
    ) -> InsertResult {
        let mut probe = ProbeSequence::new(&hash, level.size);

        for _ in 0..level.probe_limit {
            let probe_idx = probe.next();
            let entry_idx = level.start + probe_idx;
            let entry = &mut self.entries[entry_idx];

            self.stats.total_probes += 1;

            if entry.is_empty() {
                // Found empty slot
                *entry = Entry {
                    hash,
                    offset,
                    state: EntryState::Occupied,
                };
                return InsertResult::Inserted;
            } else if entry.hash == hash {
                // Found existing entry — update it
                let old_offset = entry.offset;
                entry.offset = offset;
                return InsertResult::Updated(old_offset);
            }
            // Collision — continue probing
        }

        // Probe limit exceeded — level is too full
        InsertResult::LevelFull
    }

    /// Doubles capacity and rehashes all entries.
    fn resize(&mut self) {
        let new_capacity = self.capacity() * 2;
        let mut new_table = Self::with_capacity(new_capacity);

        // Reinsert all entries
        for entry in &self.entries {
            if entry.state == EntryState::Occupied {
                new_table.insert(entry.hash, entry.offset);
            }
        }

        *self = new_table;
    }

    fn capacity(&self) -> usize {
        self.entries.len()
    }
}

enum InsertResult {
    Inserted,
    Updated(u64),
    LevelFull,
}
```

**Critical correctness properties:**
1. **Probe limit prevents infinite loops** — if level is too full, fall through to next level
2. **Resize maintains invariants** — reinserts preserve level structure
3. **Duplicate detection works** — check hash equality during probing

### 2.6 Lookup Operation (Day 7)

Implement the search logic:

```rust
// crates/core/src/algo/dedup/hash_table/elastic.rs

impl ElasticHashTable {
    /// Looks up a hash and returns the offset if found.
    pub fn get(&self, hash: &[u8; 32]) -> Option<u64> {
        self.stats.total_lookups += 1;

        // Search each level in order
        for level in &self.levels {
            if let Some(offset) = self.search_level(hash, level) {
                return Some(offset);
            }
        }

        None
    }

    /// Searches a specific level for the hash.
    fn search_level(&self, hash: &[u8; 32], level: &LevelInfo) -> Option<u64> {
        let mut probe = ProbeSequence::new(hash, level.size);

        for _ in 0..level.probe_limit {
            let probe_idx = probe.next();
            let entry_idx = level.start + probe_idx;
            let entry = &self.entries[entry_idx];

            self.stats.total_probes += 1;

            if entry.is_empty() {
                // Hash not in this level
                return None;
            } else if entry.hash == *hash {
                // Found it!
                return Some(entry.offset);
            }
            // Collision — continue probing
        }

        // Probe limit exceeded — hash not in this level
        None
    }
}
```

**Optimization notes:**
1. **Early termination on empty slot** — if we hit empty, hash can't be in this level
2. **Full memcmp for hash equality** — 32-byte comparison is fast with SIMD
3. **No bounds checking** — we know entry_idx is valid from level construction

### 2.7 Implement DedupHashTable Trait (Day 8)

Wire up the trait implementation:

```rust
// crates/core/src/algo/dedup/hash_table/elastic.rs

use super::{DedupHashTable, TableStats};

impl DedupHashTable for ElasticHashTable {
    fn insert(&mut self, hash: [u8; 32], offset: u64) -> Option<u64> {
        self.insert(hash, offset)
    }

    fn get(&self, hash: &[u8; 32]) -> Option<u64> {
        self.get(hash)
    }

    fn len(&self) -> usize {
        self.len
    }

    fn load_factor(&self) -> f64 {
        self.len as f64 / self.capacity() as f64
    }

    fn memory_bytes(&self) -> usize {
        // Entry size: 32 (hash) + 8 (offset) + 1 (state) + 7 (padding) = 48 bytes
        self.entries.capacity() * 48 +
        self.levels.capacity() * std::mem::size_of::<LevelInfo>()
    }

    fn stats(&self) -> TableStats {
        self.stats.clone()
    }
}
```

---

## Phase 3: Testing & Validation (Week 4)

### 3.1 Unit Tests (Days 1-2)

```rust
// crates/core/src/algo/dedup/hash_table/elastic.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut table = ElasticHashTable::with_capacity(16);

        let hash1 = [1u8; 32];
        let hash2 = [2u8; 32];

        assert_eq!(table.insert(hash1, 100), None);
        assert_eq!(table.insert(hash2, 200), None);

        assert_eq!(table.get(&hash1), Some(100));
        assert_eq!(table.get(&hash2), Some(200));
        assert_eq!(table.get(&[99u8; 32]), None);
    }

    #[test]
    fn test_update_existing() {
        let mut table = ElasticHashTable::with_capacity(16);
        let hash = [42u8; 32];

        assert_eq!(table.insert(hash, 100), None);
        assert_eq!(table.insert(hash, 200), Some(100));
        assert_eq!(table.get(&hash), Some(200));
    }

    #[test]
    fn test_resize() {
        let mut table = ElasticHashTable::with_capacity(16);

        // Insert until resize triggers
        for i in 0..20 {
            let mut hash = [0u8; 32];
            hash[0] = i as u8;
            table.insert(hash, i as u64);
        }

        // Verify all entries still accessible
        for i in 0..20 {
            let mut hash = [0u8; 32];
            hash[0] = i as u8;
            assert_eq!(table.get(&hash), Some(i as u64));
        }
    }

    #[test]
    fn test_high_load_factor() {
        let mut table = ElasticHashTable::with_capacity(128);

        // Fill to 0.95 load factor
        for i in 0..121 {
            let hash = blake3::hash(&i.to_le_bytes());
            table.insert(*hash.as_bytes(), i);
        }

        assert!(table.load_factor() > 0.9);

        // All entries should still be accessible
        for i in 0..121 {
            let hash = blake3::hash(&i.to_le_bytes());
            assert_eq!(table.get(hash.as_bytes()), Some(i));
        }
    }

    #[test]
    fn test_probe_limit_bounds() {
        let table = ElasticHashTable::with_capacity(1024);

        // Verify probe limit is reasonable: should be ~log log n
        let expected = (1024.0_f64.log2().log2().ceil()) as u32;

        for level in &table.levels {
            assert_eq!(level.probe_limit, expected);
        }
    }

    #[test]
    fn test_level_structure() {
        let table = ElasticHashTable::with_capacity(1024);

        // Verify geometric decrease: L0=512, L1=256, L2=128, etc.
        let mut expected_size = 512;
        for level in &table.levels {
            assert_eq!(level.size, expected_size);
            expected_size /= 2;
            if expected_size == 0 { break; }
        }
    }

    #[test]
    fn test_no_hash_collision_false_positive() {
        // Ensure we don't confuse different hashes
        let mut table = ElasticHashTable::with_capacity(16);

        let hash1 = blake3::hash(b"data1");
        let hash2 = blake3::hash(b"data2");

        table.insert(*hash1.as_bytes(), 100);

        assert_eq!(table.get(hash1.as_bytes()), Some(100));
        assert_eq!(table.get(hash2.as_bytes()), None);
    }
}
```

### 3.2 Property-Based Tests (Day 3)

Use `proptest` to verify invariants:

```rust
// Add to Cargo.toml:
// [dev-dependencies]
// proptest = "1.0"

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_insert_then_get(
            entries in prop::collection::vec((prop::array::uniform32(any::<u8>()), any::<u64>()), 1..1000)
        ) {
            let mut table = ElasticHashTable::with_capacity(entries.len() * 2);

            // Insert all entries
            for (hash, offset) in &entries {
                table.insert(*hash, *offset);
            }

            // Verify all are retrievable
            for (hash, offset) in &entries {
                assert_eq!(table.get(hash), Some(*offset));
            }
        }

        #[test]
        fn prop_no_false_positives(
            inserted in prop::collection::vec(prop::array::uniform32(any::<u8>()), 1..100),
            queried in prop::collection::vec(prop::array::uniform32(any::<u8>()), 1..100)
        ) {
            let mut table = ElasticHashTable::with_capacity(inserted.len() * 2);

            for hash in &inserted {
                table.insert(*hash, 42);
            }

            for hash in &queried {
                let result = table.get(hash);
                if !inserted.contains(hash) {
                    assert_eq!(result, None, "False positive for hash {:?}", hash);
                }
            }
        }

        #[test]
        fn prop_memory_usage_reasonable(count in 100..10000usize) {
            let mut table = ElasticHashTable::with_capacity(count);

            for i in 0..count {
                let hash = blake3::hash(&i.to_le_bytes());
                table.insert(*hash.as_bytes(), i as u64);
            }

            let memory = table.memory_bytes();
            let expected_min = count * 40; // At least 40 bytes per entry
            let expected_max = count * 60; // At most 60 bytes per entry

            assert!(memory >= expected_min && memory <= expected_max,
                "Memory {} not in range [{}, {}]", memory, expected_min, expected_max);
        }
    }
}
```

### 3.3 Benchmark Suite (Days 4-5)

Create comprehensive benchmarks comparing implementations:

```rust
// crates/cli/benches/micro/hash_table.rs

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use hexz_core::algo::dedup::hash_table::{DedupHashTable, ElasticHashTable, StandardHashTable};

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_table_insert");

    for size in [1000, 10_000, 100_000, 1_000_000] {
        group.bench_with_input(BenchmarkId::new("elastic", size), &size, |b, &size| {
            b.iter(|| {
                let mut table = ElasticHashTable::with_capacity(size);
                for i in 0..size {
                    let hash = blake3::hash(&i.to_le_bytes());
                    table.insert(*hash.as_bytes(), i as u64);
                }
            });
        });

        group.bench_with_input(BenchmarkId::new("standard", size), &size, |b, &size| {
            b.iter(|| {
                let mut table = StandardHashTable::with_capacity(size);
                for i in 0..size {
                    let hash = blake3::hash(&i.to_le_bytes());
                    table.insert(*hash.as_bytes(), i as u64);
                }
            });
        });
    }

    group.finish();
}

fn bench_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_table_lookup");

    for size in [1000, 10_000, 100_000, 1_000_000] {
        // Prepopulate tables
        let mut elastic = ElasticHashTable::with_capacity(size);
        let mut standard = StandardHashTable::with_capacity(size);

        let hashes: Vec<_> = (0..size)
            .map(|i| *blake3::hash(&i.to_le_bytes()).as_bytes())
            .collect();

        for (i, hash) in hashes.iter().enumerate() {
            elastic.insert(*hash, i as u64);
            standard.insert(*hash, i as u64);
        }

        group.bench_with_input(BenchmarkId::new("elastic", size), &size, |b, _| {
            b.iter(|| {
                for hash in &hashes {
                    black_box(elastic.get(hash));
                }
            });
        });

        group.bench_with_input(BenchmarkId::new("standard", size), &size, |b, _| {
            b.iter(|| {
                for hash in &hashes {
                    black_box(standard.get(hash));
                }
            });
        });
    }

    group.finish();
}

fn bench_high_load_factor(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_table_high_load");

    let size = 100_000;

    // Test at different load factors
    for load_pct in [70, 80, 90, 95] {
        let count = (size as f64 * load_pct as f64 / 100.0) as usize;

        group.bench_with_input(BenchmarkId::new("elastic", load_pct), &count, |b, &count| {
            b.iter(|| {
                let mut table = ElasticHashTable::with_capacity(size);
                for i in 0..count {
                    let hash = blake3::hash(&i.to_le_bytes());
                    table.insert(*hash.as_bytes(), i as u64);
                }

                // Do lookups at high load
                for i in 0..count {
                    let hash = blake3::hash(&i.to_le_bytes());
                    black_box(table.get(hash.as_bytes()));
                }
            });
        });

        group.bench_with_input(BenchmarkId::new("standard", load_pct), &count, |b, &count| {
            b.iter(|| {
                let mut table = StandardHashTable::with_capacity(size);
                for i in 0..count {
                    let hash = blake3::hash(&i.to_le_bytes());
                    table.insert(*hash.as_bytes(), i as u64);
                }

                for i in 0..count {
                    let hash = blake3::hash(&i.to_le_bytes());
                    black_box(table.get(hash.as_bytes()));
                }
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_insert, bench_lookup, bench_high_load_factor);
criterion_main!(benches);
```

---

## Phase 4: Integration (Week 5)

### 4.1 Update SnapshotWriter (Day 1)

Replace HashMap with our new abstraction:

```rust
// crates/core/src/ops/snapshot_writer.rs

use crate::algo::dedup::hash_table::{DedupHashTable, ElasticHashTable};

pub struct SnapshotWriter {
    // ... other fields ...

    // OLD: dedup_map: HashMap<[u8; 32], u64>,
    // NEW:
    dedup_map: Box<dyn DedupHashTable>,
}

impl SnapshotWriter {
    pub fn new(...) -> Self {
        Self {
            // ...
            dedup_map: Box::new(ElasticHashTable::with_capacity(4096)),
        }
    }
}
```

**Feature flag for safety:**

Add to `Cargo.toml`:
```toml
[features]
default = ["krapivin-hash"]
krapivin-hash = []
```

Then use conditional compilation:

```rust
#[cfg(feature = "krapivin-hash")]
type DedupTable = ElasticHashTable;

#[cfg(not(feature = "krapivin-hash"))]
type DedupTable = StandardHashTable;

pub struct SnapshotWriter {
    dedup_map: DedupTable,
}
```

### 4.2 Integration Tests (Days 2-3)

Test with real deduplication workloads:

```rust
// crates/core/tests/integration_hash_table.rs

#[test]
fn test_pack_with_elastic_hash_table() {
    // Create a dataset with known duplication
    let temp_dir = tempfile::tempdir().unwrap();
    let input = temp_dir.path().join("input.bin");
    let output = temp_dir.path().join("output.hxz");

    // Write 100 MB with 50% duplication (repeat blocks)
    create_duplicated_dataset(&input, 100 * 1024 * 1024, 0.5);

    // Pack with Elastic Hash Table
    let result = pack_snapshot(&input, &output, PackOptions {
        compression: CompressionType::Lz4,
        dedup: true,
        ..Default::default()
    }).unwrap();

    // Verify deduplication worked
    assert!(result.dedup_savings_pct > 40.0);
    assert!(result.dedup_savings_pct < 60.0);
}

#[test]
fn test_hash_table_memory_scaling() {
    // Test memory usage with 1M entries
    let mut table = ElasticHashTable::with_capacity(1_000_000);

    for i in 0..1_000_000 {
        let hash = blake3::hash(&i.to_le_bytes());
        table.insert(*hash.as_bytes(), i);
    }

    let memory_mb = table.memory_bytes() / (1024 * 1024);

    // Should use ~48 MB (48 bytes per entry)
    // Standard HashMap would use ~56 MB
    assert!(memory_mb >= 45 && memory_mb <= 52,
        "Memory usage {} MB out of expected range", memory_mb);
}

#[test]
fn test_compare_elastic_vs_standard() {
    let size = 100_000;

    let mut elastic = ElasticHashTable::with_capacity(size);
    let mut standard = StandardHashTable::with_capacity(size);

    // Insert same data into both
    for i in 0..size {
        let hash = blake3::hash(&i.to_le_bytes());
        elastic.insert(*hash.as_bytes(), i as u64);
        standard.insert(*hash.as_bytes(), i as u64);
    }

    // Verify identical results
    for i in 0..size {
        let hash = blake3::hash(&i.to_le_bytes());
        assert_eq!(
            elastic.get(hash.as_bytes()),
            standard.get(hash.as_bytes())
        );
    }

    // Verify memory savings
    let elastic_mem = elastic.memory_bytes();
    let standard_mem = standard.memory_bytes();

    println!("Elastic: {} MB", elastic_mem / (1024 * 1024));
    println!("Standard: {} MB", standard_mem / (1024 * 1024));

    assert!(elastic_mem < standard_mem);
}
```

### 4.3 Real-World Validation (Days 4-5)

Test with actual datasets:

```bash
# Test 1: Pack ImageNet (100K images, ~13 GB)
hexz pack --input ./imagenet100k --output imagenet.hxz --dedup

# Test 2: Pack with CDC (high deduplication scenario)
hexz pack --input ./vm-snapshots --output vms.hxz --dedup --cdc

# Test 3: Large synthetic dataset (1M blocks)
./scripts/generate_test_data.sh 1000000 > large.bin
hexz pack --input large.bin --output large.hxz --dedup

# Compare memory usage
heaptrack hexz pack --input large.bin --output test.hxz --dedup
```

---

## Phase 5: Documentation & Polish (Week 6)

### 5.1 Documentation (Days 1-2)

- [ ] Write `docs/explanation/krapivin-hash-table.md` explaining the algorithm
- [ ] Add docstrings to all public APIs
- [ ] Create ADR documenting the decision to use Elastic vs Funnel
- [ ] Update benchmarks documentation with new results
- [ ] Add example to README showcasing memory savings

### 5.2 Performance Tuning (Days 3-4)

- [ ] Profile with `perf` to find hotspots
- [ ] Optimize probe sequence generation (inline, SIMD?)
- [ ] Tune level probe limits based on real-world data
- [ ] Consider SIMD for hash comparison (compare 32 bytes at once)
- [ ] Benchmark resize strategy (when to trigger, how much to grow)

### 5.3 Safety Review (Day 5)

- [ ] Audit for undefined behavior
- [ ] Run with Miri: `MIRIFLAGS="-Zmiri-symbolic-alignment-check" cargo +nightly miri test`
- [ ] Run sanitizers: `RUSTFLAGS="-Z sanitizer=address" cargo test`
- [ ] Review all unsafe blocks (if any)
- [ ] Check for integer overflows in level calculations

### 5.4 Release Preparation (Day 6)

- [ ] Update CHANGELOG.md with v0.3.0 notes
- [ ] Create benchmark comparison graphs
- [ ] Write blog post: "First Production Implementation of Krapivin Hash Tables"
- [ ] Submit PR with full test results
- [ ] Get code review from team

---

## Success Criteria

### Correctness
- [ ] All unit tests pass
- [ ] All property tests pass with 10,000 iterations
- [ ] Integration tests pass with real datasets
- [ ] Zero failures in 24-hour stress test
- [ ] Miri and sanitizers report no issues

### Performance
- [ ] Insert throughput ≥ 90% of standard HashMap
- [ ] Lookup throughput ≥ 90% of standard HashMap
- [ ] Memory usage ≤ 85% of standard HashMap at 0.9 load factor
- [ ] Worst-case probe length ≤ 10 at 0.9 load factor

### Code Quality
- [ ] 100% documentation coverage on public APIs
- [ ] No clippy warnings
- [ ] No unsafe code (or fully justified and documented)
- [ ] Test coverage ≥ 85%

---

## Risk Mitigation

### Risk 1: Algorithm Correctness Bugs
**Mitigation:**
- Extensive property-based testing
- Side-by-side comparison with standard HashMap
- Feature flag to easily switch back to std::HashMap

### Risk 2: Performance Regression
**Mitigation:**
- Comprehensive benchmarks before/after
- CI performance regression tests
- Ability to disable via feature flag

### Risk 3: Memory Leaks
**Mitigation:**
- Run Valgrind on long-running tests
- Use heaptrack to profile memory usage
- Monitor in production with metrics

### Risk 4: Edge Cases in Paper Implementation
**Mitigation:**
- Study existing Java/Python implementations
- Test pathological inputs (all collisions, sequential inserts, etc.)
- Fuzz testing with arbitrary inputs

---

## Future Optimizations (Post-v0.3.0)

- [ ] SIMD-accelerated hash comparison
- [ ] Custom allocator for entries (avoid Vec overhead)
- [ ] Lock-free concurrent version for multi-threaded packing
- [ ] Adaptive level sizing based on data distribution
- [ ] Memory-mapped persistent dedup table for resume-after-crash

---

## References

- **Paper:** https://arxiv.org/abs/2501.02305
- **Quanta Article:** https://www.quantamagazine.org/undergraduate-upends-a-40-year-old-data-science-conjecture-20250210/
- **Java Implementation:** https://github.com/royvanrijn/optimalopen
- **Python Implementation:** https://github.com/sternma/optopenhash
- **ACM Article:** https://cacm.acm.org/news/speeding-up-hash-tables/

---

## Contact

For questions during implementation:
- Review the paper's Appendix A (detailed proofs)
- Check GitHub discussions on reference implementations
- Profile aggressively — measure, don't guess!

**Let's make Hexz the first production system to ship this breakthrough!** 🚀
