//! Cryptographic utilities for Hexz snapshot encryption.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_derivation_params_default() {
        let params = KeyDerivationParams::default();

        // Verify iterations match the constant
        assert_eq!(params.iterations, PBKDF2_ITERATIONS);

        // Verify salt has the correct size
        assert_eq!(params.salt.len(), SALT_SIZE);
    }

    #[test]
    fn test_default_salt_is_random() {
        // Generate two separate instances
        let params1 = KeyDerivationParams::default();
        let params2 = KeyDerivationParams::default();

        // Salts should be different (extremely unlikely to be equal if random)
        assert_ne!(params1.salt, params2.salt);

        // Verify salt is not all zeros (would indicate RNG failure)
        assert_ne!(params1.salt, [0u8; SALT_SIZE]);
        assert_ne!(params2.salt, [0u8; SALT_SIZE]);
    }

    #[test]
    fn test_key_derivation_params_clone() {
        let params = KeyDerivationParams::default();
        let cloned = params.clone();

        // Cloned instance should be equal
        assert_eq!(params, cloned);
        assert_eq!(params.salt, cloned.salt);
        assert_eq!(params.iterations, cloned.iterations);
    }

    #[test]
    fn test_key_derivation_params_equality() {
        let params1 = KeyDerivationParams {
            salt: [42u8; SALT_SIZE],
            iterations: 100000,
        };
        let params2 = KeyDerivationParams {
            salt: [42u8; SALT_SIZE],
            iterations: 100000,
        };
        let params3 = KeyDerivationParams {
            salt: [43u8; SALT_SIZE],
            iterations: 100000,
        };

        // Same values should be equal
        assert_eq!(params1, params2);

        // Different salt should not be equal
        assert_ne!(params1, params3);
    }

    #[test]
    fn test_key_derivation_params_serialization() {
        let params = KeyDerivationParams {
            salt: [123u8; SALT_SIZE],
            iterations: 200000,
        };

        // Serialize to JSON
        let json = serde_json::to_string(&params).expect("Serialization failed");

        // Deserialize back
        let deserialized: KeyDerivationParams =
            serde_json::from_str(&json).expect("Deserialization failed");

        // Should match original
        assert_eq!(params, deserialized);
        assert_eq!(params.salt, deserialized.salt);
        assert_eq!(params.iterations, deserialized.iterations);
    }

    #[test]
    fn test_key_derivation_params_debug() {
        let params = KeyDerivationParams {
            salt: [1u8; SALT_SIZE],
            iterations: 150000,
        };

        // Debug output should contain key information
        let debug_str = format!("{:?}", params);
        assert!(debug_str.contains("KeyDerivationParams"));
        assert!(debug_str.contains("salt"));
        assert!(debug_str.contains("iterations"));
    }

    #[test]
    fn test_iterations_constant_is_reasonable() {
        // Verify PBKDF2_ITERATIONS is in a reasonable range
        // Too low: insufficient security
        // Too high: unusable in practice
        #[allow(clippy::assertions_on_constants)]
        {
            assert!(
                PBKDF2_ITERATIONS >= 100_000,
                "Iterations too low for security"
            );
        }
        #[allow(clippy::assertions_on_constants)]
        {
            assert!(
                PBKDF2_ITERATIONS <= 10_000_000,
                "Iterations too high for usability"
            );
        }
    }

    #[test]
    fn test_salt_size_is_reasonable() {
        // Verify SALT_SIZE is sufficient for security
        // Minimum recommended: 128 bits (16 bytes)
        #[allow(clippy::assertions_on_constants)]
        {
            assert!(SALT_SIZE >= 16, "Salt size too small for security");
        }
        #[allow(clippy::assertions_on_constants)]
        {
            assert!(SALT_SIZE <= 64, "Salt size unnecessarily large");
        }
    }

    #[test]
    fn test_manual_construction() {
        let custom_salt = [99u8; SALT_SIZE];
        let custom_iterations = 500000;

        let params = KeyDerivationParams {
            salt: custom_salt,
            iterations: custom_iterations,
        };

        assert_eq!(params.salt, custom_salt);
        assert_eq!(params.iterations, custom_iterations);
    }

    #[test]
    fn test_different_defaults_have_different_salts() {
        // Create multiple default instances
        let instances: Vec<KeyDerivationParams> =
            (0..5).map(|_| KeyDerivationParams::default()).collect();

        // All salts should be unique
        for i in 0..instances.len() {
            for j in (i + 1)..instances.len() {
                assert_ne!(
                    instances[i].salt, instances[j].salt,
                    "Salts at indices {} and {} should be different",
                    i, j
                );
            }
        }
    }
}
