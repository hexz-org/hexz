//! AES-256-GCM encryption for snapshot blocks.
//!
//! Implements the `Encryptor` trait using AES-256-GCM with a PBKDF2-derived
//! key. Each block uses an implicit nonce derived from the block index.
//! Key material is derived from a user password and stored parameters.

use crate::algo::encryption::Encryptor;
use aes_gcm::{
    Aes256Gcm, Key,
    aead::{Aead, KeyInit, consts::U12, generic_array::GenericArray},
};
use hmac::Hmac;
use pbkdf2::pbkdf2;
use sha2::Sha256;
use std::fmt;
use strata_common::constants::{AES_KEY_LENGTH, AES_NONCE_LENGTH};
use strata_common::{Result, StrataError};

/// AES-256-GCM encryptor keyed via PBKDF2-derived material.
///
/// **Architectural intent:** Provides a block-indexed AEAD primitive that
/// encrypts and authenticates individual snapshot blocks using an implicit
/// nonce derived from the block number.
///
/// **Constraints:** The underlying cipher is fixed to AES-256-GCM; all blocks
/// must be decrypted with the same password, salt, and iteration count used
/// at creation time.
///
/// **Side effects:** Holds secret key material in memory for the duration of
/// its lifetime; consumers must ensure this object is not shared with
/// untrusted code.
pub struct AesGcmEncryptor {
    cipher: Aes256Gcm,
}

impl fmt::Debug for AesGcmEncryptor {
    /// Renders a redacted debug representation of the encryptor.
    ///
    /// **Architectural intent:** Avoids leaking key material or derived values
    /// into logs while still making it clear which algorithm is in use.
    ///
    /// **Constraints:** The debug output is intentionally sparse and must not
    /// be relied upon for programmatic parsing.
    ///
    /// **Side effects:** Allocates only for formatting; does not touch the
    /// underlying cipher state.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AesGcmEncryptor")
            .field("cipher", &"Aes256Gcm")
            .finish()
    }
}

impl AesGcmEncryptor {
    /// Derives an AES-256-GCM key from `password` and creates a new encryptor.
    ///
    /// **Architectural intent:** Uses PBKDF2-HMAC-SHA256 with the supplied
    /// salt and iteration count so that key derivation parameters can be
    /// stored in the snapshot header and replayed during decryption.
    ///
    /// **Constraints:** The same `(password, salt, iterations)` triple must be
    /// provided on read; iteration counts that are too low weaken resistance
    /// to brute-force, while extremely high values can make CLI usage
    /// impractically slow.
    ///
    /// **Side effects:** Performs CPU-bound key derivation proportional to
    /// `iterations` and allocates a small stack buffer for the derived key.
    pub fn new(password: &[u8], salt: &[u8], iterations: u32) -> Self {
        let mut key = [0u8; AES_KEY_LENGTH];
        pbkdf2::<Hmac<Sha256>>(password, salt, iterations, &mut key)
            .expect("HMAC can be initialized with any key length");
        let key = Key::<Aes256Gcm>::from_slice(&key);
        Self {
            cipher: Aes256Gcm::new(key),
        }
    }

    /// Computes a deterministic nonce for the given block index.
    ///
    /// **Architectural intent:** Encodes the block index into a 96-bit nonce
    /// so that every logical block uses a distinct IV under the same key,
    /// which is required for AES-GCM security.
    ///
    /// **Constraints:** The construction reserves the high 32 bits for future
    /// use and stores the big-endian `block_idx` in the low 64 bits; callers
    /// must pass the same index to `encrypt` and `decrypt` for a block.
    ///
    /// **Side effects:** Pure computation; does not touch the cipher or
    /// global state.
    fn generate_nonce(&self, block_idx: u64) -> GenericArray<u8, U12> {
        let mut bytes = [0u8; AES_NONCE_LENGTH];
        bytes[4..].copy_from_slice(&block_idx.to_be_bytes());
        *GenericArray::from_slice(&bytes)
    }
}

impl Encryptor for AesGcmEncryptor {
    /// Encrypts and authenticates a block of data using AES-256-GCM.
    ///
    /// **Architectural intent:** Produces a self-authenticating ciphertext
    /// bound to `block_idx`, allowing corruption or tampering to be detected
    /// during decryption.
    ///
    /// **Constraints:** `block_idx` must uniquely identify the logical block
    /// within the snapshot; reusing the same index with different plaintexts
    /// under the same key violates GCM security assumptions.
    ///
    /// **Side effects:** Performs symmetric cryptography and allocates a
    /// ciphertext buffer whose size is `data.len() + tag_size`.
    fn encrypt(&self, data: &[u8], block_idx: u64) -> Result<Vec<u8>> {
        let nonce = self.generate_nonce(block_idx);
        self.cipher
            .encrypt(&nonce, data)
            .map_err(|e| StrataError::Encryption(e.to_string()))
    }

    /// Decrypts and verifies a block of AES-256-GCM ciphertext.
    ///
    /// **Architectural intent:** Restores the original plaintext for the given
    /// `block_idx` and rejects any ciphertext whose authentication tag does not
    /// match, preventing silent data corruption.
    ///
    /// **Constraints:** The same `block_idx` and key material used for
    /// encryption must be supplied; mismatches will be reported as encryption
    /// errors.
    ///
    /// **Side effects:** Performs symmetric cryptography and allocates a
    /// plaintext buffer; failures are indistinguishable between corruption and
    /// incorrect keys or indices.
    fn decrypt(&self, data: &[u8], block_idx: u64) -> Result<Vec<u8>> {
        let nonce = self.generate_nonce(block_idx);
        self.cipher
            .decrypt(&nonce, data)
            .map_err(|e| StrataError::Encryption(e.to_string()))
    }
}
