//! Algorithms for compression, encryption, hashing, and deduplication.
//!
//! Unified module that groups all algorithmic primitives used by the snapshot
//! format layer. Each submodule defines a trait and one or more concrete
//! implementations that can be swapped independently.

/// Block compression codecs (LZ4, Zstd).
pub mod compression;

/// Per-block authenticated encryption (AES-256-GCM).
pub mod encryption;

/// Content-addressing hash functions.
pub mod hashing;

/// Deduplication algorithms (CDC, DCAM).
pub mod dedup;
