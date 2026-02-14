//! Encryption primitives for snapshot blocks.
//!
//! Provides the `Encryptor` trait and concrete implementations (AES-256-GCM).

use hexz_common::Result;
use std::fmt::Debug;

/// Pluggable interface for per-block authenticated encryption.
///
/// **Architectural intent:** Abstracts over concrete AEAD algorithms while
/// preserving the requirement that each logical block be independently
/// decryptable and integrity-protected.
///
/// **Constraints:** Implementations must treat `block_idx` as part of the
/// nonce or associated data so that reordering or duplication can be detected.
pub trait Encryptor: Send + Sync + Debug {
    /// Encrypts and authenticates a logical block.
    ///
    /// **Architectural intent:** Produces a ciphertext that can be stored in
    /// the snapshot data region and later verified during reads.
    ///
    /// **Constraints:** `block_idx` must uniquely identify the block within
    /// the snapshot under a given key; reusing indices can break security.
    fn encrypt(&self, data: &[u8], block_idx: u64) -> Result<Vec<u8>>;

    /// Decrypts and verifies a logical block.
    ///
    /// **Architectural intent:** Recovers the original plaintext or surfaces a
    /// hard error if tampering or key mismatch is detected.
    ///
    /// **Constraints:** Callers must pass the same `block_idx` used on
    /// encryption; failures should be treated as fatal for the affected block.
    fn decrypt(&self, data: &[u8], block_idx: u64) -> Result<Vec<u8>>;
}

/// AES-256-GCM encryption backend.
pub mod aes_gcm;

pub use aes_gcm::AesGcmEncryptor;
