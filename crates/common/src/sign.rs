//! Ed25519 signing and verification for Hexz snapshot integrity.
//!
//! Signs and verifies the master index digest to authenticate that a snapshot
//! has not been tampered with and was created by a holder of the private key.

use crate::Result;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

/// Generates an Ed25519 keypair and writes raw 32-byte keys to disk.
pub fn generate_keypair(private_out: &Path, public_out: &Path) -> Result<()> {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();

    let priv_bytes = signing_key.to_bytes();
    let pub_bytes = verifying_key.to_bytes();

    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut priv_file = opts.open(private_out)?;
    priv_file.write_all(&priv_bytes)?;

    let mut pub_file = File::create(public_out)?;
    pub_file.write_all(&pub_bytes)?;

    Ok(())
}

/// Signs a digest with an Ed25519 private key, returning a 64-byte signature.
pub fn sign_digest(private_key_path: &Path, digest: &[u8]) -> Result<[u8; 64]> {
    use zeroize::Zeroize;

    let mut f = File::open(private_key_path)?;
    let mut key_bytes = [0u8; 32];
    f.read_exact(&mut key_bytes)?;

    let signing_key = SigningKey::from_bytes(&key_bytes);
    key_bytes.zeroize();
    let signature = signing_key.sign(digest);
    Ok(signature.to_bytes())
}

/// Verifies an Ed25519 signature against a digest using a public key file.
pub fn verify_digest(
    public_key_path: &Path,
    digest: &[u8],
    signature_bytes: &[u8; 64],
) -> Result<()> {
    let mut f = File::open(public_key_path)?;
    let mut key_bytes = [0u8; 32];
    f.read_exact(&mut key_bytes)?;

    let verifying_key =
        VerifyingKey::from_bytes(&key_bytes).map_err(|e| crate::Error::Format(e.to_string()))?;
    let signature = ed25519_dalek::Signature::from_bytes(signature_bytes);
    verifying_key
        .verify(digest, &signature)
        .map_err(|e| crate::Error::Format(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::fs;

    #[test]
    fn test_generate_keypair_creates_files() {
        let temp_dir = std::env::temp_dir();
        let private_key = temp_dir.join("test_private_1.key");
        let public_key = temp_dir.join("test_public_1.pub");

        let _ = fs::remove_file(&private_key);
        let _ = fs::remove_file(&public_key);

        generate_keypair(&private_key, &public_key).expect("Keypair generation failed");

        assert!(private_key.exists());
        assert!(public_key.exists());

        let priv_metadata = fs::metadata(&private_key).unwrap();
        let pub_metadata = fs::metadata(&public_key).unwrap();
        assert_eq!(priv_metadata.len(), 32);
        assert_eq!(pub_metadata.len(), 32);

        fs::remove_file(&private_key).ok();
        fs::remove_file(&public_key).ok();
    }

    #[test]
    fn test_generate_keypair_produces_different_keys() {
        let temp_dir = std::env::temp_dir();
        let private_key1 = temp_dir.join("test_private_2.key");
        let public_key1 = temp_dir.join("test_public_2.pub");
        let private_key2 = temp_dir.join("test_private_3.key");
        let public_key2 = temp_dir.join("test_public_3.pub");

        let _ = fs::remove_file(&private_key1);
        let _ = fs::remove_file(&public_key1);
        let _ = fs::remove_file(&private_key2);
        let _ = fs::remove_file(&public_key2);

        generate_keypair(&private_key1, &public_key1).unwrap();
        generate_keypair(&private_key2, &public_key2).unwrap();

        assert_ne!(
            fs::read(&private_key1).unwrap(),
            fs::read(&private_key2).unwrap()
        );
        assert_ne!(
            fs::read(&public_key1).unwrap(),
            fs::read(&public_key2).unwrap()
        );

        fs::remove_file(&private_key1).ok();
        fs::remove_file(&public_key1).ok();
        fs::remove_file(&private_key2).ok();
        fs::remove_file(&public_key2).ok();
    }

    #[test]
    fn test_sign_and_verify_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let private_key = temp_dir.join("test_sign_private.key");
        let public_key = temp_dir.join("test_sign_public.pub");

        let _ = fs::remove_file(&private_key);
        let _ = fs::remove_file(&public_key);

        generate_keypair(&private_key, &public_key).unwrap();

        let digest = Sha256::digest(b"Important snapshot data");
        let signature = sign_digest(&private_key, &digest).unwrap();
        assert_eq!(signature.len(), 64);

        verify_digest(&public_key, &digest, &signature).unwrap();

        fs::remove_file(&private_key).ok();
        fs::remove_file(&public_key).ok();
    }

    #[test]
    fn test_sign_is_deterministic() {
        let temp_dir = std::env::temp_dir();
        let private_key = temp_dir.join("test_deterministic_private.key");
        let public_key = temp_dir.join("test_deterministic_public.pub");

        let _ = fs::remove_file(&private_key);
        let _ = fs::remove_file(&public_key);

        generate_keypair(&private_key, &public_key).unwrap();

        let digest = Sha256::digest(b"deterministic test data");
        let sig1 = sign_digest(&private_key, &digest).unwrap();
        let sig2 = sign_digest(&private_key, &digest).unwrap();
        assert_eq!(sig1, sig2);

        fs::remove_file(&private_key).ok();
        fs::remove_file(&public_key).ok();
    }

    #[test]
    fn test_verify_fails_with_tampered_data() {
        let temp_dir = std::env::temp_dir();
        let private_key = temp_dir.join("test_tamper_private.key");
        let public_key = temp_dir.join("test_tamper_public.pub");

        let _ = fs::remove_file(&private_key);
        let _ = fs::remove_file(&public_key);

        generate_keypair(&private_key, &public_key).unwrap();

        let original_digest = Sha256::digest(b"original data");
        let signature = sign_digest(&private_key, &original_digest).unwrap();

        verify_digest(&public_key, &original_digest, &signature).unwrap();

        let tampered_digest = Sha256::digest(b"tampered data");
        assert!(verify_digest(&public_key, &tampered_digest, &signature).is_err());

        fs::remove_file(&private_key).ok();
        fs::remove_file(&public_key).ok();
    }

    #[test]
    fn test_verify_fails_with_wrong_key() {
        let temp_dir = std::env::temp_dir();
        let private_key1 = temp_dir.join("test_wrongkey_private1.key");
        let public_key1 = temp_dir.join("test_wrongkey_public1.pub");
        let private_key2 = temp_dir.join("test_wrongkey_private2.key");
        let public_key2 = temp_dir.join("test_wrongkey_public2.pub");

        let _ = fs::remove_file(&private_key1);
        let _ = fs::remove_file(&public_key1);
        let _ = fs::remove_file(&private_key2);
        let _ = fs::remove_file(&public_key2);

        generate_keypair(&private_key1, &public_key1).unwrap();
        generate_keypair(&private_key2, &public_key2).unwrap();

        let digest = Sha256::digest(b"test data");
        let signature = sign_digest(&private_key1, &digest).unwrap();

        verify_digest(&public_key1, &digest, &signature).unwrap();
        assert!(verify_digest(&public_key2, &digest, &signature).is_err());

        fs::remove_file(&private_key1).ok();
        fs::remove_file(&public_key1).ok();
        fs::remove_file(&private_key2).ok();
        fs::remove_file(&public_key2).ok();
    }

    #[test]
    fn test_verify_fails_with_corrupted_signature() {
        let temp_dir = std::env::temp_dir();
        let private_key = temp_dir.join("test_corrupt_private.key");
        let public_key = temp_dir.join("test_corrupt_public.pub");

        let _ = fs::remove_file(&private_key);
        let _ = fs::remove_file(&public_key);

        generate_keypair(&private_key, &public_key).unwrap();

        let digest = Sha256::digest(b"test data");
        let mut signature = sign_digest(&private_key, &digest).unwrap();
        signature[0] ^= 0xFF;
        signature[32] ^= 0xFF;

        assert!(verify_digest(&public_key, &digest, &signature).is_err());

        fs::remove_file(&private_key).ok();
        fs::remove_file(&public_key).ok();
    }

    #[test]
    fn test_missing_key_files() {
        let temp_dir = std::env::temp_dir();
        let nonexistent = temp_dir.join("nonexistent_key_12345.key");
        let _ = fs::remove_file(&nonexistent);

        let digest = Sha256::digest(b"data");
        assert!(sign_digest(&nonexistent, &digest).is_err());
        assert!(verify_digest(&nonexistent, &digest, &[0u8; 64]).is_err());
    }

    #[test]
    fn test_wrong_size_key_files() {
        let temp_dir = std::env::temp_dir();
        let bad_key = temp_dir.join("test_bad_size.key");
        fs::write(&bad_key, [0u8; 16]).unwrap();

        let digest = Sha256::digest(b"data");
        assert!(sign_digest(&bad_key, &digest).is_err());

        fs::remove_file(&bad_key).ok();
    }

    #[test]
    fn test_different_digests_produce_different_signatures() {
        let temp_dir = std::env::temp_dir();
        let private_key = temp_dir.join("test_diff_sigs_private.key");
        let public_key = temp_dir.join("test_diff_sigs_public.pub");

        let _ = fs::remove_file(&private_key);
        let _ = fs::remove_file(&public_key);

        generate_keypair(&private_key, &public_key).unwrap();

        let sig1 = sign_digest(&private_key, &Sha256::digest(b"data 1")).unwrap();
        let sig2 = sign_digest(&private_key, &Sha256::digest(b"data 2")).unwrap();
        assert_ne!(sig1, sig2);

        fs::remove_file(&private_key).ok();
        fs::remove_file(&public_key).ok();
    }
}
