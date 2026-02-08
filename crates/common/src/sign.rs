//! Cryptographic signing and verification utilities using Ed25519.

use crate::Result;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Generates a new Ed25519 keypair and writes the private/public keys to disk.
pub fn generate_keypair(private_out: &Path, public_out: &Path) -> Result<()> {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();

    // Write private key (raw bytes for now, or PEM if we added pem crate)
    // For simplicity, we'll write raw bytes or hex. Let's use PEM-like wrapper or raw.
    // The plan said "PEM or custom format". We'll stick to raw bytes for simplicity in MVP
    // or hex encoded for readability. Let's use Hex.
    let priv_bytes = signing_key.to_bytes();
    let pub_bytes = verifying_key.to_bytes();

    let mut priv_file = File::create(private_out)?;
    priv_file.write_all(&priv_bytes)?;

    let mut pub_file = File::create(public_out)?;
    pub_file.write_all(&pub_bytes)?;

    Ok(())
}

/// Signs a digest with a private key loaded from a file.
pub fn sign_digest(private_key_path: &Path, digest: &[u8]) -> Result<[u8; 64]> {
    let mut f = File::open(private_key_path)?;
    let mut key_bytes = [0u8; 32];
    f.read_exact(&mut key_bytes)?;

    let signing_key = SigningKey::from_bytes(&key_bytes);
    let signature = signing_key.sign(digest);
    Ok(signature.to_bytes())
}

/// Verifies a signature against a digest using a public key from a file.
pub fn verify_digest(
    public_key_path: &Path,
    digest: &[u8],
    signature_bytes: &[u8; 64],
) -> Result<()> {
    let mut f = File::open(public_key_path)?;
    let mut key_bytes = [0u8; 32];
    f.read_exact(&mut key_bytes)?;

    let verifying_key = VerifyingKey::from_bytes(&key_bytes)
        .map_err(|e| crate::StrataError::Format(e.to_string()))?;
    let signature = ed25519_dalek::Signature::from_bytes(signature_bytes);
    verifying_key
        .verify(digest, &signature)
        .map_err(|e| crate::StrataError::Format(e.to_string()))?;
    Ok(())
}

/// Computes the SHA256 digest of the master index (and relevant header parts) of a snapshot.
///
/// **Implementation Note:** This reads the file to find the index and computes the hash.
/// It mimics `StrataFile` loading but focuses on the index range.
pub fn compute_snapshot_digest(path: &Path) -> Result<[u8; 32]> {
    let mut f = File::open(path)?;
    // Read header to find index offset
    // Note: HEADER_SIZE constant is now in strata-core::format::magic (4096 bytes)
    let mut header_bytes = [0u8; 4096];
    f.read_exact(&mut header_bytes)?;

    // We ideally should deserialize header, but since `strata_core` depends on `strata_common`,
    // we can't depend on `strata_core` here. We have to parse the offset manually or move this logic
    // to `strata_cli` or `strata_core`.
    // Move this logic to `strata_cli` which has access to `strata_core`.
    // Only keeping keygen/sign/verify primitives here.

    Err(crate::StrataError::Format(
        "Digest computation should be done in CLI/Core layer".into(),
    ))
}
