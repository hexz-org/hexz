//! Unit tests for encryption/decryption (AES-256-GCM).
//!
//! Tests verify correct encryption round-trips, nonce generation,
//! tamper detection, and error handling.

use super::common;
use common::*;

use hexz_core::algo::encryption::{Encryptor, aes_gcm::AesGcmEncryptor};

/// Test basic encryption/decryption round-trip.
#[test]
fn test_aes_gcm_roundtrip() {
    let password = b"test_password_123";
    let salt = b"random_salt_1234";
    let iterations = 100_000;

    let encryptor = AesGcmEncryptor::new(password, salt, iterations).unwrap();

    let plaintext = b"Hello, World! This is a test message.";
    let block_idx = 0;

    let ciphertext = encryptor
        .encrypt(plaintext, block_idx)
        .expect("Encryption failed");

    // Ciphertext should be different from plaintext
    assert_ne!(&ciphertext[..], plaintext);

    // Ciphertext should be longer (includes authentication tag)
    assert!(ciphertext.len() > plaintext.len());

    let decrypted = encryptor
        .decrypt(&ciphertext, block_idx)
        .expect("Decryption failed");

    assert_eq!(decrypted, plaintext);
}

/// Test that different block indices produce different ciphertexts.
#[test]
fn test_block_index_affects_encryption() {
    let password = b"password";
    let salt = b"salt12345678";
    let encryptor = AesGcmEncryptor::new(password, salt, 100_000).unwrap();

    let plaintext = b"Same plaintext";

    let cipher0 = encryptor.encrypt(plaintext, 0).unwrap();
    let cipher1 = encryptor.encrypt(plaintext, 1).unwrap();
    let cipher2 = encryptor.encrypt(plaintext, 2).unwrap();

    // Different block indices should produce different ciphertexts
    assert_ne!(cipher0, cipher1);
    assert_ne!(cipher1, cipher2);
    assert_ne!(cipher0, cipher2);

    // But all should decrypt correctly
    assert_eq!(encryptor.decrypt(&cipher0, 0).unwrap(), plaintext);
    assert_eq!(encryptor.decrypt(&cipher1, 1).unwrap(), plaintext);
    assert_eq!(encryptor.decrypt(&cipher2, 2).unwrap(), plaintext);
}

/// Test that wrong block index fails decryption.
#[test]
fn test_wrong_block_index_fails() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt_16_bytes___", 100_000).unwrap();
    let plaintext = b"Test data";

    let ciphertext = encryptor.encrypt(plaintext, 5).unwrap();

    // Decrypting with wrong block index should fail
    let result = encryptor.decrypt(&ciphertext, 6);
    assert!(result.is_err());
}

/// Test that different passwords produce different results.
#[test]
fn test_different_passwords() {
    let salt = b"shared_salt_pad_";
    let plaintext = b"Sensitive data";
    let block_idx = 0;

    let enc1 = AesGcmEncryptor::new(b"password1", salt, 100_000).unwrap();
    let enc2 = AesGcmEncryptor::new(b"password2", salt, 100_000).unwrap();

    let cipher1 = enc1.encrypt(plaintext, block_idx).unwrap();
    let cipher2 = enc2.encrypt(plaintext, block_idx).unwrap();

    // Different passwords should produce different ciphertexts
    assert_ne!(cipher1, cipher2);

    // enc1 cannot decrypt enc2's ciphertext
    let result = enc1.decrypt(&cipher2, block_idx);
    assert!(result.is_err());

    // But each can decrypt their own
    assert_eq!(enc1.decrypt(&cipher1, block_idx).unwrap(), plaintext);
    assert_eq!(enc2.decrypt(&cipher2, block_idx).unwrap(), plaintext);
}

/// Test that different salts produce different results.
#[test]
fn test_different_salts() {
    let password = b"same_password";
    let plaintext = b"Data";
    let block_idx = 0;

    let enc1 = AesGcmEncryptor::new(password, b"salt1___pad_", 100_000).unwrap();
    let enc2 = AesGcmEncryptor::new(password, b"salt2___pad_", 100_000).unwrap();

    let cipher1 = enc1.encrypt(plaintext, block_idx).unwrap();
    let cipher2 = enc2.encrypt(plaintext, block_idx).unwrap();

    // Different salts should produce different ciphertexts
    assert_ne!(cipher1, cipher2);

    // Cross-decryption should fail
    assert!(enc1.decrypt(&cipher2, block_idx).is_err());
    assert!(enc2.decrypt(&cipher1, block_idx).is_err());
}

/// Test encryption of empty data.
#[test]
fn test_encrypt_empty() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt_16_bytes___", 100_000).unwrap();
    let empty: &[u8] = &[];

    let ciphertext = encryptor.encrypt(empty, 0).unwrap();

    // Ciphertext should contain authentication tag even for empty data
    assert!(!ciphertext.is_empty());

    let decrypted = encryptor.decrypt(&ciphertext, 0).unwrap();
    assert_eq!(decrypted, empty);
}

/// Test encryption of large data block.
#[test]
fn test_encrypt_large_block() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt_16_bytes___", 100_000).unwrap();
    let large_data = vec![0x42u8; 65536]; // 64KB

    let ciphertext = encryptor.encrypt(&large_data, 0).unwrap();

    // Ciphertext should be slightly larger (tag overhead)
    assert!(ciphertext.len() > large_data.len());
    assert!(ciphertext.len() < large_data.len() + 100);

    let decrypted = encryptor.decrypt(&ciphertext, 0).unwrap();
    assert_eq!(decrypted, large_data);
}

/// Test tamper detection - modify ciphertext.
#[test]
fn test_tamper_detection_modify() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt_16_bytes___", 100_000).unwrap();
    let plaintext = b"Important data";

    let mut ciphertext = encryptor.encrypt(plaintext, 0).unwrap();

    // Tamper with the ciphertext
    if ciphertext.len() > 10 {
        ciphertext[10] ^= 0xFF;

        // Decryption should fail due to authentication failure
        let result = encryptor.decrypt(&ciphertext, 0);
        assert!(result.is_err());
    }
}

/// Test tamper detection - truncate ciphertext.
#[test]
fn test_tamper_detection_truncate() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt_16_bytes___", 100_000).unwrap();
    let plaintext = b"Test message";

    let ciphertext = encryptor.encrypt(plaintext, 0).unwrap();

    // Truncate the ciphertext
    let truncated = &ciphertext[..ciphertext.len() - 5];

    // Decryption should fail
    let result = encryptor.decrypt(truncated, 0);
    assert!(result.is_err());
}

/// Test encryption with various PBKDF2 iteration counts.
#[test]
fn test_various_iteration_counts() {
    let password = b"password";
    let salt = b"salt_16_bytes___";
    let plaintext = b"Data";
    let block_idx = 0;

    let counts = vec![100_000, 200_000, 600_000];

    for iterations in counts {
        let encryptor = AesGcmEncryptor::new(password, salt, iterations).unwrap();
        let ciphertext = encryptor.encrypt(plaintext, block_idx).unwrap();
        let decrypted = encryptor.decrypt(&ciphertext, block_idx).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}

/// Test that same password/salt but different iteration counts fail cross-decryption.
#[test]
fn test_iteration_count_affects_key() {
    let password = b"password";
    let salt = b"salt_16_bytes___";
    let plaintext = b"Data";
    let block_idx = 0;

    let enc1 = AesGcmEncryptor::new(password, salt, 100_000).unwrap();
    let enc2 = AesGcmEncryptor::new(password, salt, 200_000).unwrap();

    let cipher1 = enc1.encrypt(plaintext, block_idx).unwrap();

    // Different iteration counts produce different keys
    let result = enc2.decrypt(&cipher1, block_idx);
    assert!(result.is_err());
}

/// Test encryption with maximum block index.
#[test]
fn test_max_block_index() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt_16_bytes___", 100_000).unwrap();
    let plaintext = b"Data";

    let max_idx = u64::MAX;
    let ciphertext = encryptor.encrypt(plaintext, max_idx).unwrap();
    let decrypted = encryptor.decrypt(&ciphertext, max_idx).unwrap();

    assert_eq!(decrypted, plaintext);
}

/// Test multiple encryptions with sequential block indices.
#[test]
fn test_sequential_block_indices() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt_16_bytes___", 100_000).unwrap();
    let plaintext = b"Block data";

    for idx in 0..100 {
        let ciphertext = encryptor.encrypt(plaintext, idx).unwrap();
        let decrypted = encryptor.decrypt(&ciphertext, idx).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}

/// Test that authentication tag prevents silent corruption.
#[test]
fn test_authentication_prevents_corruption() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt_16_bytes___", 100_000).unwrap();
    let plaintext = b"Critical data that must not be corrupted";

    let mut ciphertext = encryptor.encrypt(plaintext, 0).unwrap();

    // Flip a single bit in the middle
    if ciphertext.len() > 20 {
        let mid = ciphertext.len() / 2;
        ciphertext[mid] ^= 0x01;

        // Decryption must fail - no silent corruption allowed
        let result = encryptor.decrypt(&ciphertext, 0);
        assert!(
            result.is_err(),
            "Authentication should detect single-bit corruption"
        );
    }
}

// Nonce/IV handling tests

#[test]
fn test_nonce_uniqueness_sequential_blocks() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt12345678", 100_000).unwrap();
    let plaintext = b"Test data";

    // Encrypt blocks 0, 1, 2 sequentially
    let cipher0 = encryptor.encrypt(plaintext, 0).unwrap();
    let cipher1 = encryptor.encrypt(plaintext, 1).unwrap();
    let cipher2 = encryptor.encrypt(plaintext, 2).unwrap();

    // All should be different
    assert_ne!(cipher0, cipher1);
    assert_ne!(cipher1, cipher2);
    assert_ne!(cipher0, cipher2);

    // All should decrypt correctly
    assert_eq!(encryptor.decrypt(&cipher0, 0).unwrap(), plaintext);
    assert_eq!(encryptor.decrypt(&cipher1, 1).unwrap(), plaintext);
    assert_eq!(encryptor.decrypt(&cipher2, 2).unwrap(), plaintext);
}

#[test]
fn test_sparse_block_indices() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt12345678", 100_000).unwrap();
    let plaintext = b"Test data";

    // Use sparse block indices
    let indices = [0, 1000000, 2000000, 5000000];

    let mut ciphertexts = Vec::new();
    for &idx in &indices {
        let cipher = encryptor.encrypt(plaintext, idx).unwrap();
        ciphertexts.push((idx, cipher));
    }

    // All should be unique
    for i in 0..ciphertexts.len() {
        for j in (i + 1)..ciphertexts.len() {
            assert_ne!(ciphertexts[i].1, ciphertexts[j].1);
        }
    }

    // All should decrypt correctly
    for (idx, cipher) in &ciphertexts {
        assert_eq!(encryptor.decrypt(cipher, *idx).unwrap(), plaintext);
    }
}

#[test]
fn test_maximum_block_index() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt12345678", 100_000).unwrap();
    let plaintext = b"Test at max index";

    // Test with very large block index (but not u64::MAX which might be reserved)
    let max_idx = u64::MAX - 1;
    let cipher = encryptor.encrypt(plaintext, max_idx).unwrap();
    let decrypted = encryptor.decrypt(&cipher, max_idx).unwrap();

    assert_eq!(decrypted, plaintext);
}

// Tampering detection tests

#[test]
fn test_bitflip_in_auth_tag() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt12345678", 100_000).unwrap();
    let plaintext = b"Sensitive data";

    let mut ciphertext = encryptor.encrypt(plaintext, 0).unwrap();

    // Flip a bit in the last byte (auth tag area)
    let last_idx = ciphertext.len() - 1;
    ciphertext[last_idx] ^= 0x01;

    // Decryption should fail
    let result = encryptor.decrypt(&ciphertext, 0);
    assert!(
        result.is_err(),
        "Modified auth tag should fail authentication"
    );
}

#[test]
fn test_swap_encrypted_blocks() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt12345678", 100_000).unwrap();

    let plaintext1 = b"Block 1 data";
    let plaintext2 = b"Block 2 data";

    let cipher1 = encryptor.encrypt(plaintext1, 0).unwrap();
    let cipher2 = encryptor.encrypt(plaintext2, 1).unwrap();

    // Try to decrypt cipher1 with block_idx=1 (wrong index)
    let result = encryptor.decrypt(&cipher1, 1);
    assert!(result.is_err(), "Swapped block should fail authentication");

    // Try to decrypt cipher2 with block_idx=0 (wrong index)
    let result = encryptor.decrypt(&cipher2, 0);
    assert!(result.is_err(), "Swapped block should fail authentication");
}

#[test]
fn test_truncate_encrypted_data() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt12345678", 100_000).unwrap();
    let plaintext = b"This is a longer message that will be encrypted";

    let mut ciphertext = encryptor.encrypt(plaintext, 0).unwrap();

    // Truncate the ciphertext
    ciphertext.truncate(ciphertext.len() / 2);

    // Decryption should fail
    let result = encryptor.decrypt(&ciphertext, 0);
    assert!(result.is_err(), "Truncated ciphertext should fail");
}

// Key derivation tests

#[test]
fn test_different_passwords_different_keys() {
    let salt = b"same_salt_123456";
    let plaintext = b"Test message";

    let enc1 = AesGcmEncryptor::new(b"password1", salt, 100_000).unwrap();
    let enc2 = AesGcmEncryptor::new(b"password2", salt, 100_000).unwrap();

    let cipher1 = enc1.encrypt(plaintext, 0).unwrap();
    let cipher2 = enc2.encrypt(plaintext, 0).unwrap();

    // Different passwords should produce different ciphertexts
    assert_ne!(cipher1, cipher2);

    // Cross-decryption should fail
    assert!(enc1.decrypt(&cipher2, 0).is_err());
    assert!(enc2.decrypt(&cipher1, 0).is_err());
}

#[test]
fn test_same_password_different_salt() {
    let password = b"same_password";
    let plaintext = b"Test message";

    let enc1 = AesGcmEncryptor::new(password, b"salt1_123456", 100_000).unwrap();
    let enc2 = AesGcmEncryptor::new(password, b"salt2_654321", 100_000).unwrap();

    let cipher1 = enc1.encrypt(plaintext, 0).unwrap();
    let cipher2 = enc2.encrypt(plaintext, 0).unwrap();

    // Different salts should produce different ciphertexts even with same password
    assert_ne!(cipher1, cipher2);

    // Cross-decryption should fail
    assert!(enc1.decrypt(&cipher2, 0).is_err());
    assert!(enc2.decrypt(&cipher1, 0).is_err());
}

#[test]
fn test_pbkdf2_iteration_count() {
    let password = b"password";
    let salt = b"salt12345678";
    let plaintext = b"Test message";

    let enc_low = AesGcmEncryptor::new(password, salt, 100_000).unwrap();
    let enc_high = AesGcmEncryptor::new(password, salt, 200_000).unwrap();

    let cipher_low = enc_low.encrypt(plaintext, 0).unwrap();
    let cipher_high = enc_high.encrypt(plaintext, 0).unwrap();

    // Different iteration counts should produce different keys/ciphertexts
    assert_ne!(cipher_low, cipher_high);

    // Cross-decryption should fail
    assert!(enc_low.decrypt(&cipher_high, 0).is_err());
    assert!(enc_high.decrypt(&cipher_low, 0).is_err());
}

// Performance and large data tests

#[test]
fn test_encrypt_large_block_1mb() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt12345678", 100_000).unwrap();
    let plaintext = create_random_data(1024 * 1024); // 1MB

    let ciphertext = encryptor.encrypt(&plaintext, 0).unwrap();
    let decrypted = encryptor.decrypt(&ciphertext, 0).unwrap();

    assert_bytes_equal(&decrypted, &plaintext, "1MB encryption round-trip");
}

#[test]
fn test_encrypt_large_block_10mb() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt12345678", 100_000).unwrap();
    let plaintext = create_random_data(10 * 1024 * 1024); // 10MB

    let ciphertext = encryptor.encrypt(&plaintext, 0).unwrap();
    let decrypted = encryptor.decrypt(&ciphertext, 0).unwrap();

    assert_bytes_equal(&decrypted, &plaintext, "10MB encryption round-trip");
}

#[test]
fn test_batch_encrypt_many_blocks() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt12345678", 100_000).unwrap();

    // Encrypt 100 small blocks
    for i in 0..100 {
        let plaintext = format!("Block number {}", i).into_bytes();
        let ciphertext = encryptor.encrypt(&plaintext, i as u64).unwrap();
        let decrypted = encryptor.decrypt(&ciphertext, i as u64).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}

// Edge case tests

#[test]
fn test_encrypt_empty_data() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt12345678", 100_000).unwrap();
    let plaintext = b"";

    let ciphertext = encryptor.encrypt(plaintext, 0).unwrap();
    let decrypted = encryptor.decrypt(&ciphertext, 0).unwrap();

    assert_eq!(decrypted, plaintext);
    // Even empty data should have an auth tag
    assert!(!ciphertext.is_empty());
}

#[test]
fn test_encrypt_single_byte() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt12345678", 100_000).unwrap();
    let plaintext = b"X";

    let ciphertext = encryptor.encrypt(plaintext, 0).unwrap();
    let decrypted = encryptor.decrypt(&ciphertext, 0).unwrap();

    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_encrypt_pattern_data() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt12345678", 100_000).unwrap();

    // All zeros
    let zeros = vec![0u8; 1000];
    let cipher_zeros = encryptor.encrypt(&zeros, 0).unwrap();
    let decrypted_zeros = encryptor.decrypt(&cipher_zeros, 0).unwrap();
    assert_eq!(decrypted_zeros, zeros);

    // All ones
    let ones = vec![0xFFu8; 1000];
    let cipher_ones = encryptor.encrypt(&ones, 1).unwrap();
    let decrypted_ones = encryptor.decrypt(&cipher_ones, 1).unwrap();
    assert_eq!(decrypted_ones, ones);
}

#[test]
fn test_reuse_same_block_index() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt12345678", 100_000).unwrap();

    let plaintext1 = b"First encryption";
    let plaintext2 = b"Second encryption";

    // Encrypt different data with same block index
    let cipher1 = encryptor.encrypt(plaintext1, 42).unwrap();
    let cipher2 = encryptor.encrypt(plaintext2, 42).unwrap();

    // Ciphertexts should be different (different plaintext)
    assert_ne!(cipher1, cipher2);

    // Both should decrypt correctly
    assert_eq!(encryptor.decrypt(&cipher1, 42).unwrap(), plaintext1);
    assert_eq!(encryptor.decrypt(&cipher2, 42).unwrap(), plaintext2);
}

#[test]
fn test_very_long_password() {
    let long_password = vec![b'a'; 1000];
    let encryptor = AesGcmEncryptor::new(&long_password, b"salt12345678", 100_000).unwrap();

    let plaintext = b"Test with long password";
    let ciphertext = encryptor.encrypt(plaintext, 0).unwrap();
    let decrypted = encryptor.decrypt(&ciphertext, 0).unwrap();

    assert_eq!(decrypted, plaintext);
}

#[test]
fn test_deterministic_encryption_same_inputs() {
    let encryptor = AesGcmEncryptor::new(b"password", b"salt12345678", 100_000).unwrap();
    let plaintext = b"Deterministic test";

    // Encrypt same data with same block index multiple times
    let cipher1 = encryptor.encrypt(plaintext, 100).unwrap();
    let cipher2 = encryptor.encrypt(plaintext, 100).unwrap();

    // Should produce identical ciphertexts (deterministic with same nonce)
    assert_eq!(cipher1, cipher2);
}

/// Test that short salt is rejected.
#[test]
fn test_short_salt_rejected() {
    let result = AesGcmEncryptor::new(b"password", b"short", 100_000);
    assert!(result.is_err());
}

/// Test that low iteration count is rejected.
#[test]
fn test_low_iterations_rejected() {
    let result = AesGcmEncryptor::new(b"password", b"salt_16_bytes___", 1000);
    assert!(result.is_err());
}
