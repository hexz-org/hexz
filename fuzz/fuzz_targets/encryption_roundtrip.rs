#![no_main]
use arbitrary::Arbitrary;
use hexz_core::algo::encryption::{AesGcmEncryptor, Encryptor};
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
struct Input {
    /// Password bytes (1..64 bytes)
    password: Vec<u8>,
    /// Block data to encrypt (1..4096 bytes)
    data: Vec<u8>,
    /// Block index
    block_idx: u64,
}

fuzz_target!(|input: Input| {
    // Need non-empty password and data
    if input.password.is_empty() || input.data.is_empty() {
        return;
    }

    // Clamp sizes to avoid excessive allocation
    let password = &input.password[..input.password.len().min(64)];
    let data = &input.data[..input.data.len().min(4096)];

    // Use fixed salt and low iterations for fuzzing speed
    let salt = b"fuzz_salt_16byte";
    let iterations = 100_000; // minimum allowed

    let encryptor = match AesGcmEncryptor::new(password, salt, iterations) {
        Ok(e) => e,
        Err(_) => return,
    };

    // Encrypt
    let encrypted = match encryptor.encrypt(data, input.block_idx) {
        Ok(e) => e,
        Err(_) => return,
    };

    // Decrypt must roundtrip
    let decrypted = match encryptor.decrypt(&encrypted, input.block_idx) {
        Ok(d) => d,
        Err(_) => {
            // If encryption succeeded, decryption with same key+idx must succeed
            panic!("decrypt failed after successful encrypt");
        }
    };

    assert_eq!(data, decrypted.as_slice(), "roundtrip mismatch");

    // Decrypting with wrong block_idx should fail
    if input.block_idx != input.block_idx.wrapping_add(1) {
        let wrong_idx = input.block_idx.wrapping_add(1);
        let result = encryptor.decrypt(&encrypted, wrong_idx);
        // Should be an error (authentication failure), not a panic
        assert!(result.is_err(), "decrypt with wrong block_idx should fail");
    }
});
