//! Cryptographic utilities for Strata snapshot encryption.
//!
//! Defines key-derivation parameters (PBKDF2 salt and iteration count) used
//! when creating or opening encrypted snapshots. These parameters are
//! serialized into snapshot metadata so that the same password reproduces
//! the same key on restore.

use crate::constants::{PBKDF2_ITERATIONS, SALT_SIZE};
use serde::{Deserialize, Serialize};

/// Parameters for deriving an encryption key from a user-supplied secret.
///
/// **Architectural intent:** Encapsulates the tunable inputs for PBKDF2-based
/// key derivation so that snapshot metadata fully describes how to reproduce
/// the encryption key on restore.
///
/// **Constraints:** `salt` must be unique per snapshot to avoid key reuse, and
/// `iterations` is chosen to be intentionally expensive (hundreds of thousands
/// of rounds) to raise the cost of offline brute-force attacks while remaining
/// acceptable for interactive CLI usage.
///
/// **Side effects:** Instances produced via `Default` consume randomness from
/// the process RNG and implicitly fix the work factor at the compile-time
/// constant embedded in this type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyDerivationParams {
    pub salt: [u8; SALT_SIZE],
    pub iterations: u32,
}

impl Default for KeyDerivationParams {
    /// Generates key-derivation parameters for a new snapshot.
    ///
    /// **Architectural intent:** Produces a fresh random salt and a stable
    /// iteration count that all writers and readers agree on, so that keys can
    /// be recomputed deterministically from the password and stored metadata.
    ///
    /// **Constraints:** The salt is 128 bits and sampled uniformly from the
    /// system RNG; the iteration count is fixed to a value calibrated for this
    /// application and must be kept in sync with password prompts.
    ///
    /// **Side effects:** Pulls entropy from `rand::thread_rng` and performs no
    /// I/O; increasing the iteration count will linearly increase CPU cost for
    /// both snapshot creation and decryption.
    fn default() -> Self {
        let mut salt = [0u8; SALT_SIZE];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
        Self {
            salt,
            iterations: PBKDF2_ITERATIONS,
        }
    }
}
