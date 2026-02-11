//! Cryptographic signing and verification utilities using Ed25519.

use crate::Result;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Generates a new Ed25519 keypair and writes the private/public keys to disk.
///
/// This function creates a cryptographically secure Ed25519 keypair using the
/// operating system's random number generator (`OsRng`). The private and public
/// keys are written as raw 32-byte binary files.
///
/// # Security Considerations
///
/// - **Private key storage**: The private key is written to disk without encryption.
///   Callers should ensure appropriate file permissions (e.g., chmod 600) and
///   consider using OS keychain services for production use.
/// - **Key backup**: Loss of the private key means signatures cannot be created.
///   Consider implementing a secure backup strategy.
/// - **Key rotation**: For long-term use, implement periodic key rotation.
///
/// # Arguments
///
/// * `private_out` - Path where the private key (32 bytes) will be written
/// * `public_out` - Path where the public key (32 bytes) will be written
///
/// # Returns
///
/// Returns `Ok(())` on success, or an error if file creation fails.
///
/// # Errors
///
/// This function returns an error if:
/// - File creation fails (permission denied, disk full, etc.)
/// - Write operations fail (I/O errors)
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use strata_common::sign::generate_keypair;
///
/// # fn main() -> strata_common::Result<()> {
/// // Generate a new keypair
/// let private_key = Path::new("snapshot.key");
/// let public_key = Path::new("snapshot.pub");
///
/// generate_keypair(private_key, public_key)?;
///
/// // Set restrictive permissions on private key (Unix only)
/// #[cfg(unix)]
/// {
///     use std::fs;
///     use std::os::unix::fs::PermissionsExt;
///     let mut perms = fs::metadata(private_key)?.permissions();
///     perms.set_mode(0o600);
///     fs::set_permissions(private_key, perms)?;
/// }
/// # Ok(())
/// # }
/// ```
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
///
/// This function loads an Ed25519 private key from disk and uses it to sign
/// the provided digest. The signature can later be verified using the
/// corresponding public key via [`verify_digest`].
///
/// # Security Considerations
///
/// - **Digest input**: The caller must provide a cryptographic hash (e.g., SHA-256)
///   of the data to sign, not the raw data itself. See [`compute_snapshot_digest`]
///   for snapshot-specific hashing.
/// - **Key protection**: Private key is read from disk without encryption.
/// - **Signature properties**: Ed25519 signatures are deterministic and do not
///   require additional randomness.
///
/// # Arguments
///
/// * `private_key_path` - Path to the private key file (32 bytes, raw binary)
/// * `digest` - The digest to sign (typically 32 bytes for SHA-256)
///
/// # Returns
///
/// Returns a 64-byte Ed25519 signature on success.
///
/// # Errors
///
/// This function returns an error if:
/// - The private key file cannot be read ([`StrataError::Io`])
/// - The file is not exactly 32 bytes ([`StrataError::Io`])
///
/// [`StrataError::Io`]: crate::StrataError::Io
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use sha2::{Sha256, Digest};
/// use strata_common::sign::sign_digest;
///
/// # fn main() -> strata_common::Result<()> {
/// // Compute SHA-256 digest of data
/// let data = b"snapshot contents";
/// let mut hasher = Sha256::new();
/// hasher.update(data);
/// let digest = hasher.finalize();
///
/// // Sign the digest
/// let private_key = Path::new("snapshot.key");
/// let signature = sign_digest(private_key, &digest)?;
///
/// println!("Signature: {} bytes", signature.len());
/// # Ok(())
/// # }
/// ```
pub fn sign_digest(private_key_path: &Path, digest: &[u8]) -> Result<[u8; 64]> {
    let mut f = File::open(private_key_path)?;
    let mut key_bytes = [0u8; 32];
    f.read_exact(&mut key_bytes)?;

    let signing_key = SigningKey::from_bytes(&key_bytes);
    let signature = signing_key.sign(digest);
    Ok(signature.to_bytes())
}

/// Verifies a signature against a digest using a public key from a file.
///
/// This function loads an Ed25519 public key from disk and uses it to verify
/// that the provided signature was created by the corresponding private key
/// over the given digest.
///
/// # Security Considerations
///
/// - **Signature verification**: Verification failure does not distinguish between
///   invalid signatures and corrupted data. Both cases return an error.
/// - **Public key trust**: This function does not establish trust in the public key
///   itself. Callers must verify public key authenticity through out-of-band means
///   (e.g., certificate chains, key fingerprints).
///
/// # Arguments
///
/// * `public_key_path` - Path to the public key file (32 bytes, raw binary)
/// * `digest` - The digest that was signed (typically 32 bytes for SHA-256)
/// * `signature_bytes` - The 64-byte Ed25519 signature to verify
///
/// # Returns
///
/// Returns `Ok(())` if the signature is valid, or an error otherwise.
///
/// # Errors
///
/// This function returns an error if:
/// - The public key file cannot be read ([`StrataError::Io`])
/// - The file is not exactly 32 bytes ([`StrataError::Io`])
/// - The public key bytes are invalid ([`StrataError::Format`])
/// - The signature verification fails ([`StrataError::Format`])
///
/// [`StrataError::Io`]: crate::StrataError::Io
/// [`StrataError::Format`]: crate::StrataError::Format
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use sha2::{Sha256, Digest};
/// use strata_common::sign::{sign_digest, verify_digest};
///
/// # fn main() -> strata_common::Result<()> {
/// let data = b"snapshot contents";
/// let mut hasher = Sha256::new();
/// hasher.update(data);
/// let digest = hasher.finalize();
///
/// // Sign with private key
/// let signature = sign_digest(Path::new("snapshot.key"), &digest)?;
///
/// // Verify with public key
/// let public_key = Path::new("snapshot.pub");
/// verify_digest(public_key, &digest, &signature)?;
///
/// println!("Signature verified successfully");
/// # Ok(())
/// # }
/// ```
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
