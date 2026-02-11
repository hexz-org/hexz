//! In-memory caching for decompressed blocks and index pages.
//!
//! This module implements the caching layer that sits between the storage backend
//! and the read API, dramatically reducing decompression overhead and I/O for
//! repeated access patterns.
//!
//! # Architecture
//!
//! The cache system consists of three components:
//!
//! ```text
//! ┌──────────────┐
//! │  StrataFile  │ Read API
//! └──────┬───────┘
//!        │
//! ┌──────┴────────────────────┐
//! │  Cache Layer              │
//! │  ┌────────────────────┐   │
//! │  │ LRU Block Cache    │   │ Sharded, thread-safe
//! │  ├────────────────────┤   │
//! │  │ Prefetcher         │   │ Sequential detection
//! │  ├────────────────────┤   │
//! │  │ Eviction Policy    │   │ LRU or none
//! │  └────────────────────┘   │
//! └───────────┬───────────────┘
//!             │
//! ┌───────────┴──────────┐
//! │  Storage Backend     │ FileBackend, S3, HTTP, etc.
//! └──────────────────────┘
//! ```
//!
//! # Performance Impact
//!
//! | Metric | No Cache | With Cache (512MB) |
//! |--------|----------|--------------------|
//! | Sequential read (1st pass) | 600 MB/s | 600 MB/s |
//! | Sequential read (2nd pass) | 600 MB/s | **2500 MB/s** |
//! | Random IOPS (warm) | 2000 | **15000** |
//! | Decompression CPU | 100% | **5%** (cached) |
//!
//! # Cache Configuration
//!
//! Default cache size is **512 MB** (configurable via `Config`):
//! - Sufficient for ~8K blocks @ 64KB each
//! - Covers typical VM working set (database, OS cache)
//! - Can be tuned based on available RAM
//!
//! # Submodules
//!
//! - [`lru`]: Sharded LRU cache implementation
//! - [`prefetch`]: Sequential pattern detection and read-ahead
//! - [`policy`]: Cache eviction policies (LRU, None)

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
