//! Sharded block cache and default capacity configuration.
//!
//! Provides the L1 block cache used to avoid repeated decompression and
//! backend reads, and the default page-cache size used by index consumers.
//! Capacity and shard counts are fixed at compile time; runtime tuning is
//! not exposed.

use crate::api::stratafile::SnapshotStream;
use bytes::Bytes;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;

/// Default capacity for the L1 block cache (number of entries).
const DEFAULT_L1_CAPACITY: usize = 1000;

/// Default capacity for the Index Page cache (number of entries).
const DEFAULT_PAGE_CACHE_CAPACITY: usize = 128;

/// Cache key used by `BlockCache`.
///
/// The first component encodes the logical snapshot stream (`SnapshotStream`
/// discriminant as `u8`), and the second component is the zero-based block
/// index within that stream.
type CacheKey = (u8, u64);

#[derive(Debug)]
/// Sharded LRU cache for decompressed snapshot blocks.
///
/// **Architectural intent:** Reduces repeated decompression and backend reads
/// by caching hot blocks in memory, while sharding to minimize lock
/// contention under concurrent access.
///
/// **Constraints:** For capacity > SHARD_COUNT, total capacity is divided
/// evenly across shards. For 0 < capacity <= SHARD_COUNT, a single shard is
/// used so global LRU semantics and exact capacity are preserved.
///
/// **Side effects:** Uses per-shard `Mutex`es, so high write or miss rates can
/// introduce contention; memory usage grows with the number and size of cached
/// blocks up to the configured capacity.
pub struct BlockCache {
    /// None = capacity 0 (no-op cache). Some = one or more shards.
    shards: Option<Vec<Mutex<LruCache<CacheKey, Bytes>>>>,
}

/// Number of independent shards in the `BlockCache`.
///
/// **Architectural intent:** Trades a modest fixed overhead for reduced
/// mutex contention by partitioning the key space; 16 shards is a balance
/// tuned for typical multi-core hosts.
///
/// **Constraints:** Must be a power-of-two-like small integer to keep shard
/// sizing simple; changing it affects cache behavior and hit rate profiles.
const SHARD_COUNT: usize = 16;

/// Capacity below which we use a single shard for exact LRU and simpler behavior.
const SINGLE_SHARD_CAPACITY_LIMIT: usize = 256;

impl BlockCache {
    /// Constructs a new block cache with a target total capacity in entries.
    ///
    /// **Architectural intent:** Evenly partitions `capacity` across shards so
    /// that each shard maintains its own LRU without requiring a global lock.
    ///
    /// **Constraints:** `capacity` is expressed in number of blocks, not
    /// bytes; very small capacities are clamped so each shard holds at least
    /// one entry.
    ///
    /// **Side effects:** Allocates shard metadata and underlying LRU
    /// structures; no I/O is performed.
    pub fn with_capacity(capacity: usize) -> Self {
        if capacity == 0 {
            return Self { shards: None };
        }
        let (num_shards, cap_per_shard) = if capacity <= SINGLE_SHARD_CAPACITY_LIMIT {
            // Single shard so total capacity is exact and LRU is global
            (1, NonZeroUsize::new(capacity).unwrap())
        } else {
            // Ceiling division so total capacity >= capacity
            let cap_per = (capacity + SHARD_COUNT - 1) / SHARD_COUNT;
            (SHARD_COUNT, NonZeroUsize::new(cap_per.max(1)).unwrap())
        };
        let mut shards = Vec::with_capacity(num_shards);
        for _ in 0..num_shards {
            shards.push(Mutex::new(LruCache::new(cap_per_shard)));
        }
        Self {
            shards: Some(shards),
        }
    }

    /// Selects the shard responsible for a given cache key.
    ///
    /// **Architectural intent:** Uses a stable hash of the `CacheKey` to
    /// distribute entries across shards and avoid hot-spotting a single lock.
    ///
    /// **Constraints:** The hash function must remain deterministic within a
    /// process; changing it will reshuffle shard assignments and invalidate
    /// any intuitive reasoning about locality.
    fn get_shard(&self, key: &CacheKey) -> Option<&Mutex<LruCache<CacheKey, Bytes>>> {
        let shards = self.shards.as_ref()?;
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let idx = (hasher.finish() as usize) % shards.len();
        Some(&shards[idx])
    }

    /// Looks up a decompressed block in the cache, if present.
    ///
    /// **Architectural intent:** Fast-path for reads when the requested block
    /// is already resident, avoiding backend I/O and decompression.
    ///
    /// **Constraints:** Uses the `SnapshotStream` discriminant as the stream
    /// identifier; callers must pass the same stream mapping used when
    /// inserting.
    ///
    /// **Side effects:** Acquires a mutex on the corresponding shard and may
    /// clone the underlying `Bytes` buffer on a hit.
    pub fn get(&self, stream: SnapshotStream, block: u64) -> Option<Bytes> {
        let key = (stream as u8, block);
        let shard = self.get_shard(&key)?;
        shard.lock().ok()?.get(&key).cloned()
    }

    /// Inserts or updates a decompressed block in the cache.
    ///
    /// **Architectural intent:** Records the most recently used representation
    /// of a block so that subsequent reads can be served from memory.
    ///
    /// **Constraints:** The caller is responsible for ensuring that `data`
    /// corresponds exactly to the logical block `(stream, block)`; stale
    /// entries are not invalidated automatically.
    ///
    /// **Side effects:** Acquires a mutex and may evict the least-recently
    /// used entry in the shard; the eviction policy is purely LRU.
    pub fn insert(&self, stream: SnapshotStream, block: u64, data: Bytes) {
        let key = (stream as u8, block);
        if let Some(shard) = self.get_shard(&key) {
            if let Ok(mut guard) = shard.lock() {
                guard.put(key, data);
            }
        }
    }
}

impl Default for BlockCache {
    /// Constructs a block cache with the default L1 capacity.
    ///
    /// **Architectural intent:** Allows `BlockCache` to be used as a drop-in
    /// default without callers specifying capacity; matches typical
    /// single-snapshot workloads.
    ///
    /// **Side effects:** Equivalent to `BlockCache::with_capacity(DEFAULT_L1_CAPACITY)`;
    /// no I/O is performed.
    fn default() -> Self {
        Self::with_capacity(DEFAULT_L1_CAPACITY)
    }
}

/// Returns the default index page cache capacity in number of entries.
///
/// **Architectural intent:** Provides a single compile-time default for
/// index page caches so that consumers that do not configure capacity still
/// get a bounded, non-zero size.
///
/// **Constraints:** The value is fixed (128 entries); callers requiring
/// different capacities must obtain `NonZeroUsize` elsewhere.
pub fn default_page_cache_size() -> NonZeroUsize {
    NonZeroUsize::new(DEFAULT_PAGE_CACHE_CAPACITY).unwrap()
}
