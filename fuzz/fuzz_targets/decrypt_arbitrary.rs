#![no_main]
use hexz_core::algo::encryption::{AesGcmEncryptor, Encryptor};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fixed encryptor — we're testing that arbitrary ciphertext never panics
    let encryptor = match AesGcmEncryptor::new(b"fuzzpassword", b"fuzz_salt_16byte", 100_000) {
        Ok(e) => e,
        Err(_) => return,
    };

    // Decrypt arbitrary bytes with various block indices — should return Err, never panic
    let _ = encryptor.decrypt(data, 0);
    let _ = encryptor.decrypt(data, 1);
    let _ = encryptor.decrypt(data, u64::MAX);

    // Also test decrypt_into
    let mut out = Vec::new();
    let _ = encryptor.decrypt_into(data, 0, &mut out);
});
