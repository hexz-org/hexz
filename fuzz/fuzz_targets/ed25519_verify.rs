#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
struct Input {
    key_bytes: [u8; 32],
    signature_bytes: [u8; 64],
    digest: Vec<u8>,
}

fuzz_target!(|input: Input| {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    // Constructing a VerifyingKey from arbitrary bytes — should not panic
    let key = match VerifyingKey::from_bytes(&input.key_bytes) {
        Ok(k) => k,
        Err(_) => return,
    };

    // Constructing a Signature from arbitrary bytes — should not panic
    let sig = Signature::from_bytes(&input.signature_bytes);

    // Verification with arbitrary inputs — should not panic
    let _ = key.verify(&input.digest, &sig);
});
