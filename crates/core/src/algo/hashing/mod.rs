//! Content-addressing hash primitives.
//!
//! Provides a `ContentHasher` trait for computing content-defined hashes
//! used in deduplication and integrity verification.

use hexz_common::Result;

/// Trait for content-addressing hash functions.
///
/// **Architectural intent:** Abstracts over concrete hash algorithms so that
/// the deduplication and integrity layers can swap hash functions without
/// changing call sites.
pub trait ContentHasher: Send + Sync {
    /// Returns the hash of the given data as a fixed-size byte array.
    ///
    /// **Performance Note:** This method allocates a Vec on every call.
    /// For hot paths, use `hash_into` instead to reuse a buffer.
    fn hash(&self, data: &[u8]) -> Result<Vec<u8>>;

    /// Computes the hash and writes it into the provided buffer.
    ///
    /// **Arguments:**
    /// - `data`: The data to hash
    /// - `out`: Output buffer (must be at least `output_len()` bytes)
    ///
    /// **Returns:** The number of bytes written (always `output_len()`)
    ///
    /// **Performance:** Reuses the provided buffer, avoiding allocations.
    /// Prefer this in hot paths (e.g., deduplication loops).
    fn hash_into(&self, data: &[u8], out: &mut [u8]) -> Result<usize>;

    /// Returns the expected output length in bytes.
    fn output_len(&self) -> usize;
}

pub mod blake3;
