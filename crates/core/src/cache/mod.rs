//! In-memory caching for decompressed blocks and index pages.
//!
//! Provides the block cache (sharded LRU), eviction policies, prefetch
//! logic, and default capacity configuration. Used by `StrataFile` to
//! reduce repeated decompression and backend reads.

/// Cache admission and eviction strategies.
///
/// Contains policies that decide which blocks should be retained or evicted
/// when cache capacity is constrained.
pub mod policy;

/// Background and anticipatory prefetch logic.
///
/// Implements heuristics for reading future blocks ahead of demand to hide
/// storage latency for sequential and patterned workloads.
pub mod prefetch;

/// LRU cache implementation for decompressed data and index pages.
///
/// Maintains sharded LRU structures used by `StrataFile` for efficient
/// concurrent access and cache eviction.
pub mod lru;
