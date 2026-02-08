//! Cache Eviction Policy Definitions.
//!
//! This module defines the algorithms used to determine which blocks should
//! be removed from the cache when it reaches its capacity limit.
//! Proper eviction policies are crucial for maximizing cache hit rates
//! and ensuring efficient memory utilization.

/// Enumerates the supported cache eviction strategies.
///
/// This enum allows the system to select different behaviors for memory management.
/// Currently, it supports a standard Least Recently Used (LRU) policy and a
/// "None" policy for cases where memory is unbounded or managed externally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionPolicy {
    /// Evicts the least recently accessed items first.
    ///
    /// This is the standard policy for general-purpose caching, ensuring that
    /// frequently accessed "hot" data remains in memory while "cold" data is discarded.
    Lru,

    /// No eviction policy; the cache grows indefinitely.
    ///
    /// This mode should be used with caution, typically only for small datasets
    /// that fit entirely in RAM or for debugging purposes. It will eventually
    /// cause an Out Of Memory (OOM) error if the working set exceeds available memory.
    None,
}
