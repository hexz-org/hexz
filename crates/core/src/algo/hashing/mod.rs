//! Content-addressing hash primitives.
//!
//! Provides a `ContentHasher` trait for computing content-defined hashes
//! used in deduplication and integrity verification.

use strata_common::Result;

/// Trait for content-addressing hash functions.
///
/// **Architectural intent:** Abstracts over concrete hash algorithms so that
/// the deduplication and integrity layers can swap hash functions without
/// changing call sites.
pub trait ContentHasher: Send + Sync {
    /// Returns the hash of the given data as a fixed-size byte array.
    fn hash(&self, data: &[u8]) -> Result<Vec<u8>>;

    /// Returns the expected output length in bytes.
    fn output_len(&self) -> usize;
}

pub mod blake3;
