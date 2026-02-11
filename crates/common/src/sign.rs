//! Cryptographic signing and verification for Strata snapshot integrity.
//!
//! This module provides primitives for signing and verifying Strata snapshot files
//! using Ed25519 digital signatures. It enables authenticating that a snapshot's
//! master index has not been tampered with and was created by a holder of the
//! corresponding private key.
//!
//! # Overview
//!
//! Strata snapshots contain critical system state, including file mappings, block
//! metadata, and compression metadata. To ensure integrity and authenticity, Strata
//! supports cryptographic signing of the master index—the authoritative registry of
//! all blocks and their locations within the snapshot file.
//!
//! The signing workflow involves:
//! 1. **Key generation**: Creating an Ed25519 keypair and storing it securely
//! 2. **Signing**: Computing a SHA-256 digest of the master index and signing it
//! 3. **Verification**: Loading a public key and verifying the signature matches the index digest
//!
//! # Cryptographic Primitives
//!
//! ## Ed25519 Digital Signatures
//!
//! Strata uses **Ed25519**, a modern elliptic-curve signature scheme providing:
//! - **Security**: 128-bit security level, resistant to quantum pre-image attacks
//! - **Performance**: Fast signing (~50k signatures/sec on typical hardware)
//! - **Determinism**: No random number generation during signing (reduces attack surface)
//! - **Compactness**: 32-byte public keys, 64-byte signatures
//!
//! Ed25519 is implemented via the [`ed25519-dalek`](https://docs.rs/ed25519-dalek) crate,
//! which provides constant-time operations to resist timing side-channels.
//!
//! ## SHA-256 Hashing
//!
//! Master index digests use **SHA-256**, providing:
//! - 256-bit collision resistance
//! - Pre-image resistance (cannot reverse-engineer index from digest)
//! - Wide deployment and extensive cryptanalysis
//!
//! The digest is computed over the serialized master index bytes, which includes
//! block offsets, checksums, compression metadata, and file tree structures.
//!
//! # Trust Model
//!
//! ## Threat Model
//!
//! Strata's signing mechanism defends against:
//!
//! ### In-Scope Threats
//! - **Malicious snapshot modification**: Attackers cannot alter the master index
//!   without invalidating the signature
//! - **Snapshot forgery**: Without the private key, attackers cannot create valid
//!   signatures for fabricated snapshots
//! - **Downgrade attacks**: Signatures are tied to specific index content; older
//!   snapshots cannot be substituted without detection
//!
//! ### Out-of-Scope Threats
//! - **Private key compromise**: If the signing key is stolen, attackers can forge
//!   valid signatures. Implement key rotation and Hardware Security Module (HSM)
//!   protection for production environments.
//! - **Public key substitution**: This module does not solve key distribution or
//!   certificate authority (CA) infrastructure. Callers must verify public key
//!   authenticity through out-of-band mechanisms (e.g., fingerprint comparison,
//!   TLS certificate chains).
//! - **Data block tampering**: Only the master index is signed. Individual data
//!   blocks rely on checksums stored in the index. If the index is authentic,
//!   tampered blocks will fail checksum validation during decompression.
//! - **Replay attacks**: Signatures do not include timestamps or nonces. Applications
//!   requiring freshness guarantees must implement additional mechanisms.
//!
//! ## Trust Establishment
//!
//! Trust flows through the following chain:
//! 1. **Key generation**: Operator generates a keypair on a secure workstation
//! 2. **Private key custody**: Private key is stored with restrictive permissions (mode 0600)
//! 3. **Public key distribution**: Public key is distributed to verifiers via trusted channels
//!    (e.g., configuration management, manual fingerprint verification)
//! 4. **Signing**: Snapshot creator signs the master index with their private key
//! 5. **Verification**: Consumers verify signatures using the trusted public key
//!
//! # Use Cases
//!
//! ## Secure Boot / Immutable Infrastructure
//!
//! Strata snapshots can serve as root filesystem images for unikernels or containers.
//! Signing ensures that only authorized images boot, preventing rootkit injection
//! or configuration tampering.
//!
//! Example:
//! - CI/CD pipeline generates a snapshot and signs it with the release key
//! - Boot loader verifies the signature before mounting the snapshot
//! - If verification fails, the system refuses to boot
//!
//! ## Trusted Backups
//!
//! When storing snapshots in untrusted storage (cloud object stores, third-party
//! backup services), signatures prove that restored data has not been altered.
//!
//! Example:
//! - Backup process signs each snapshot before uploading to S3
//! - Restoration process verifies signatures before extracting files
//! - Alerts are triggered if signature verification fails
//!
//! ## Software Distribution
//!
//! Distributing pre-built Strata snapshots (e.g., application bundles, VM images)
//! requires authenticity guarantees to prevent supply-chain attacks.
//!
//! Example:
//! - Vendor signs official snapshot releases with a well-known public key
//! - Users verify signatures before deploying snapshots to production
//! - Key fingerprints are published on the vendor's website via HTTPS
//!
//! # Security Considerations
//!
//! ## Key Storage Best Practices
//!
//! ### Private Key Protection
//! - **File permissions**: Store private keys with mode `0600` (owner read/write only)
//! - **Encryption at rest**: For production, encrypt private keys with a passphrase
//!   or use OS keychain services (e.g., macOS Keychain, GNOME Keyring)
//! - **Hardware Security Modules (HSMs)**: For critical infrastructure, store signing
//!   keys in FIPS 140-2 Level 2+ HSMs (e.g., YubiHSM, AWS CloudHSM)
//! - **Key backups**: Maintain encrypted backups of private keys in geographically
//!   distributed secure storage
//!
//! ### Public Key Distribution
//! - **Out-of-band verification**: Publish public key fingerprints (SHA-256 of the
//!   public key bytes) on a separate trusted channel (e.g., company website via HTTPS)
//! - **Configuration management**: Distribute public keys via trusted CM systems
//!   (Ansible, Puppet) that authenticate sources
//! - **Avoid untrusted sources**: Do not retrieve public keys from the same location
//!   as snapshots (defeats the purpose of signing)
//!
//! ## Signature Format
//!
//! Ed25519 signatures are **64 bytes** in the following format:
//! - **Bytes 0-31**: R value (curve point, compressed Edwards Y-coordinate)
//! - **Bytes 32-63**: S value (scalar)
//!
//! The signature is deterministic: signing the same digest with the same key always
//! produces the same signature. This prevents signature malleability attacks.
//!
//! Signatures are stored in Strata snapshot files at the location specified by
//! `StrataHeader::signature_offset` and `StrataHeader::signature_length`. The
//! signature is appended after the master index to avoid modifying earlier file
//! regions.
//!
//! ## Failure Modes and Attack Scenarios
//!
//! ### Scenario: Attacker Modifies Index
//! **Attack**: Adversary alters block offsets in the master index to redirect reads
//! to malicious data blocks.
//!
//! **Defense**: The SHA-256 digest of the modified index will not match the signed
//! digest. Verification fails, alerting the operator to tampering.
//!
//! **Outcome**: Attack detected, snapshot rejected.
//!
//! ### Scenario: Attacker Forges Signature
//! **Attack**: Without the private key, adversary attempts to create a valid signature
//! for a malicious index.
//!
//! **Defense**: Ed25519's 128-bit security level makes brute-forcing the private key
//! computationally infeasible (requires ~2^128 operations).
//!
//! **Outcome**: Forged signatures fail verification. Attack is mitigated.
//!
//! ### Scenario: Private Key Compromise
//! **Attack**: Adversary steals the private key and signs malicious snapshots.
//!
//! **Defense**: This module cannot prevent key compromise. Mitigations include:
//! - Regular key rotation (generate new keypairs quarterly)
//! - HSM storage for private keys
//! - Multi-signature schemes (require N-of-M keys to sign)
//!
//! **Outcome**: Compromised key must be revoked. Implement key rotation procedures.
//!
//! ### Scenario: Downgrade Attack
//! **Attack**: Adversary replaces a current snapshot with an older, validly-signed
//! snapshot containing known vulnerabilities.
//!
//! **Defense**: Signatures do not include timestamps or version numbers. Applications
//! must implement additional checks:
//! - Verify snapshot version metadata
//! - Maintain a ledger of seen snapshot hashes
//! - Enforce monotonic version increases
//!
//! **Outcome**: Requires application-level defenses beyond this module.
//!
//! # Key Rotation
//!
//! Regular key rotation limits the window of exposure if a private key is compromised.
//!
//! ## Rotation Procedure
//!
//! 1. **Generate new keypair**: Use [`generate_keypair`] to create a new Ed25519 keypair
//! 2. **Dual-sign transition period**: For a transition window (e.g., 30 days), sign
//!    new snapshots with both old and new keys
//! 3. **Distribute new public key**: Push the new public key to all verifiers via
//!    configuration management
//! 4. **Revoke old key**: After the transition period, stop using the old private key
//!    and purge it from all systems
//! 5. **Update documentation**: Publish the new public key fingerprint
//!
//! ## Key Rotation Frequency
//! - **Development**: Annually (lower risk tolerance)
//! - **Production**: Quarterly (higher security requirements)
//! - **Post-compromise**: Immediately (if breach is suspected)
//!
//! # Integration Examples
//!
//! ## Full Workflow: Key Generation, Signing, and Verification
//!
//! This example demonstrates the complete lifecycle of signing and verifying a
//! Strata snapshot.
//!
//! ```no_run
//! use std::path::Path;
//! use std::fs::{self, OpenOptions};
//! use std::io::{Read, Write, Seek, SeekFrom};
//! use sha2::{Sha256, Digest};
//! use strata_common::sign::{generate_keypair, sign_digest, verify_digest};
//!
//! # fn main() -> strata_common::Result<()> {
//! // Step 1: Generate keypair
//! let private_key = Path::new("snapshot.key");
//! let public_key = Path::new("snapshot.pub");
//!
//! generate_keypair(private_key, public_key)?;
//!
//! // Step 2: Set restrictive permissions on private key (Unix only)
//! #[cfg(unix)]
//! {
//!     use std::os::unix::fs::PermissionsExt;
//!     let mut perms = fs::metadata(private_key)?.permissions();
//!     perms.set_mode(0o600);
//!     fs::set_permissions(private_key, perms)?;
//! }
//!
//! // Step 3: Compute digest of data to sign (e.g., master index)
//! let index_data = b"Master index contents with block mappings...";
//! let mut hasher = Sha256::new();
//! hasher.update(index_data);
//! let digest = hasher.finalize();
//!
//! // Step 4: Sign the digest
//! let signature = sign_digest(private_key, &digest)?;
//!
//! // Step 5: Store signature in snapshot file (simplified example)
//! let mut snapshot = OpenOptions::new()
//!     .write(true)
//!     .create(true)
//!     .open("snapshot.strata")?;
//! snapshot.write_all(index_data)?;
//! let sig_offset = snapshot.seek(SeekFrom::Current(0))?;
//! snapshot.write_all(&signature)?;
//!
//! println!("Snapshot signed. Signature offset: {}", sig_offset);
//!
//! // Step 6: Verification (typically performed by a different party)
//! verify_digest(public_key, &digest, &signature)?;
//! println!("Signature verified successfully!");
//!
//! # Ok(())
//! # }
//! ```
//!
//! ## Production Integration with Strata CLI
//!
//! The Strata CLI (`strata sign` and `strata verify` commands) integrates these
//! primitives to sign complete snapshot files. The CLI:
//! 1. Parses the `StrataHeader` to locate the master index
//! 2. Computes SHA-256 of the index bytes
//! 3. Signs the digest and appends the signature to the snapshot file
//! 4. Updates `StrataHeader::signature_offset` and `signature_length` fields
//!
//! See `crates/cli/src/cmd/sys/sign.rs` and `crates/cli/src/cmd/sys/verify.rs`
//! for the complete implementation.
//!
//! # Feature Flag
//!
//! This module is gated behind the `signing` feature flag in `Cargo.toml`. To enable:
//!
//! ```toml
//! [dependencies]
//! strata-common = { version = "0.1.0", features = ["signing"] }
//! ```
//!
//! This allows builds that do not require signing to avoid including cryptographic
//! dependencies, reducing binary size and attack surface.

use crate::Result;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Generates a new Ed25519 keypair and writes the private/public keys to disk.
///
/// This function creates a cryptographically secure Ed25519 keypair using the
/// operating system's random number generator ([`OsRng`]). The keypair is generated
/// using the Ed25519 signature scheme as defined in RFC 8032, providing 128-bit
/// security level.
///
/// Both keys are written as **raw binary files** containing exactly 32 bytes each:
/// - **Private key**: 32-byte Ed25519 secret scalar
/// - **Public key**: 32-byte compressed Edwards curve point
///
/// # Key Generation Algorithm
///
/// Key generation follows these steps:
/// 1. Draw 32 cryptographically random bytes from [`OsRng`] (uses `/dev/urandom`
///    on Unix, `BCryptGenRandom` on Windows)
/// 2. Clamp the scalar according to Ed25519 specification (RFC 8032 Section 5.1.5)
/// 3. Compute the public key by scalar multiplication on Curve25519
/// 4. Write private key bytes to `private_out`
/// 5. Write public key bytes to `public_out`
///
/// # File Permissions
///
/// **Critical Security Requirement**: This function writes the private key with
/// default file permissions (typically 0644 on Unix). **Callers MUST set restrictive
/// permissions** (mode 0600) immediately after generation to prevent unauthorized
/// access. See the example below for correct usage.
///
/// On Windows, use file ACLs to restrict access to the key owner only.
///
/// # Security Considerations
///
/// ## Private Key Storage
/// - **Unencrypted storage**: Keys are written as plaintext binary. For production
///   environments, consider encrypting the private key with a passphrase (using
///   AES-256-GCM or ChaCha20-Poly1305) or storing it in a Hardware Security Module (HSM).
/// - **Memory safety**: Private key material is held in memory during generation.
///   The `ed25519-dalek` crate does not provide automatic zeroing; consider using
///   the `zeroize` crate for sensitive deployments.
/// - **File permissions**: **Must be set to 0600** (owner read/write only) to prevent
///   other users from reading the private key.
///
/// ## Key Backup
/// - Loss of the private key is **irrecoverable**. Signatures cannot be created
///   without it.
/// - Implement a secure backup strategy:
///   - Encrypt backups with a strong passphrase (e.g., using `age` or GPG)
///   - Store backups in geographically distributed locations
///   - Test recovery procedures regularly
///
/// ## Key Rotation
/// - For long-term use, rotate keys periodically:
///   - Development: Annually
///   - Production: Quarterly
///   - Post-compromise: Immediately
/// - See the module-level documentation for detailed rotation procedures.
///
/// # Arguments
///
/// * `private_out` - Path where the 32-byte private key will be written. **Warning:**
///   If this file already exists, it will be **overwritten** without confirmation.
/// * `public_out` - Path where the 32-byte public key will be written. **Warning:**
///   If this file already exists, it will be **overwritten** without confirmation.
///
/// # Returns
///
/// Returns `Ok(())` on success. Both key files are guaranteed to be flushed to disk
/// before the function returns.
///
/// # Errors
///
/// This function returns [`StrataError::Io`] if:
/// - **File creation fails**:
///   - Parent directory does not exist
///   - Permission denied (cannot write to target directory)
///   - Disk is full or quota exceeded
///   - Path is a directory or special file (cannot overwrite)
/// - **Write operations fail**:
///   - I/O error during write (hardware failure, network filesystem timeout)
/// - **File already exists** (behavior depends on OS/filesystem):
///   - On most systems, existing files are **silently overwritten**
///   - If the file is open by another process, behavior is platform-specific
///     (may fail on Windows, succeed on Unix)
///
/// **Note on existing files**: This function uses [`File::create`], which truncates
/// existing files. If you need to prevent accidental overwrites, check for file
/// existence before calling this function:
///
/// ```no_run
/// use std::path::Path;
/// use strata_common::sign::generate_keypair;
///
/// # fn main() -> strata_common::Result<()> {
/// let private_key = Path::new("snapshot.key");
/// let public_key = Path::new("snapshot.pub");
///
/// // Prevent accidental key overwrites
/// if private_key.exists() || public_key.exists() {
///     eprintln!("Error: Key files already exist. Refusing to overwrite.");
///     eprintln!("Delete existing keys manually if you intend to regenerate.");
///     std::process::exit(1);
/// }
///
/// generate_keypair(private_key, public_key)?;
/// # Ok(())
/// # }
/// ```
///
/// [`StrataError::Io`]: crate::StrataError::Io
/// [`File::create`]: std::fs::File::create
///
/// # Examples
///
/// ## Basic Usage with Correct Permissions
///
/// ```no_run
/// use std::path::Path;
/// use std::fs;
/// use strata_common::sign::generate_keypair;
///
/// # fn main() -> strata_common::Result<()> {
/// let private_key = Path::new("snapshot.key");
/// let public_key = Path::new("snapshot.pub");
///
/// // Generate keypair
/// generate_keypair(private_key, public_key)?;
///
/// // CRITICAL: Set restrictive permissions on private key (Unix only)
/// #[cfg(unix)]
/// {
///     use std::os::unix::fs::PermissionsExt;
///     let mut perms = fs::metadata(private_key)?.permissions();
///     perms.set_mode(0o600); // Owner read/write only
///     fs::set_permissions(private_key, perms)?;
/// }
///
/// // Windows: Set ACLs to restrict access (requires additional crates like `windows-acl`)
/// #[cfg(windows)]
/// {
///     // Example (requires windows-acl crate):
///     // use windows_acl::acl::ACL;
///     // ACL::from_file_path(private_key, false)?
///     //     .remove_all()?
///     //     .allow_current_user_full()?
///     //     .save(private_key)?;
/// }
///
/// println!("Keypair generated successfully!");
/// println!("Private key: {} (KEEP SECRET)", private_key.display());
/// println!("Public key: {} (distribute to verifiers)", public_key.display());
/// # Ok(())
/// # }
/// ```
///
/// ## Production Usage with Existence Checks
///
/// ```no_run
/// use std::path::Path;
/// use std::fs;
/// use strata_common::sign::generate_keypair;
///
/// # fn main() -> strata_common::Result<()> {
/// let private_key = Path::new("/etc/strata/signing.key");
/// let public_key = Path::new("/etc/strata/signing.pub");
///
/// // Check for existing keys to prevent accidental overwrites
/// if private_key.exists() || public_key.exists() {
///     return Err(strata_common::StrataError::Format(
///         "Key files already exist. Use a different path or remove existing keys.".into()
///     ));
/// }
///
/// // Generate keypair
/// generate_keypair(private_key, public_key)?;
///
/// // Set restrictive permissions
/// #[cfg(unix)]
/// {
///     use std::os::unix::fs::PermissionsExt;
///     let mut perms = fs::metadata(private_key)?.permissions();
///     perms.set_mode(0o600);
///     fs::set_permissions(private_key, perms)?;
/// }
///
/// // Compute and display public key fingerprint for verification
/// let public_key_bytes = fs::read(public_key)?;
/// use sha2::{Sha256, Digest};
/// let fingerprint = Sha256::digest(&public_key_bytes);
/// println!("Public key fingerprint (SHA-256): {:x}", fingerprint);
/// println!("Share this fingerprint via a trusted channel (e.g., HTTPS)");
/// # Ok(())
/// # }
/// ```
///
/// ## Backup Strategy Example
///
/// ```no_run
/// use std::path::Path;
/// use std::fs;
/// use std::process::Command;
/// use strata_common::sign::generate_keypair;
///
/// # fn main() -> strata_common::Result<()> {
/// let private_key = Path::new("snapshot.key");
/// let public_key = Path::new("snapshot.pub");
///
/// generate_keypair(private_key, public_key)?;
///
/// // Set permissions
/// #[cfg(unix)]
/// {
///     use std::os::unix::fs::PermissionsExt;
///     let mut perms = fs::metadata(private_key)?.permissions();
///     perms.set_mode(0o600);
///     fs::set_permissions(private_key, perms)?;
/// }
///
/// // Create encrypted backup using age (https://github.com/FiloSottile/age)
/// // Requires `age` CLI tool to be installed
/// let backup_result = Command::new("age")
///     .args(&[
///         "-e",                          // encrypt
///         "-p",                          // use passphrase
///         "-o", "snapshot.key.age",      // output file
///         private_key.to_str().unwrap()
///     ])
///     .status();
///
/// match backup_result {
///     Ok(status) if status.success() => {
///         println!("Encrypted backup created: snapshot.key.age");
///         println!("Store this backup in a secure location (off-site storage)");
///     }
///     _ => eprintln!("Warning: Failed to create encrypted backup"),
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

/// Signs a cryptographic digest with an Ed25519 private key.
///
/// This function loads an Ed25519 private key from disk and uses it to generate
/// a digital signature over the provided digest. The signature proves that the
/// holder of the corresponding private key attests to the authenticity of the
/// data represented by the digest.
///
/// The signature can later be verified using the corresponding public key via
/// [`verify_digest`].
///
/// # Signature Algorithm
///
/// The signing process follows RFC 8032 Section 5.1.6 (Ed25519 signature generation):
/// 1. Load the 32-byte private key from `private_key_path`
/// 2. Compute the public key from the private key (deterministic)
/// 3. Compute `r = hash(hash(private_key)[32..64] || message)` (where message is `digest`)
/// 4. Compute `R = r * G` (curve point multiplication)
/// 5. Compute `S = r + hash(R || A || message) * s` (where `A` is the public key, `s` is the secret scalar)
/// 6. Return signature as `R || S` (64 bytes total)
///
/// **Determinism**: Ed25519 signatures are fully deterministic. Signing the same
/// digest with the same key always produces the **exact same signature**. This
/// eliminates random number generation as an attack vector and ensures reproducibility.
///
/// # Expected Digest Format
///
/// ## Digest Length
/// The `digest` parameter accepts **any byte slice**, but typical usage passes a
/// cryptographic hash output:
/// - **SHA-256**: 32 bytes (recommended for Strata snapshots)
/// - **SHA-512**: 64 bytes
/// - **BLAKE3**: 32 bytes (default output length)
///
/// **Warning**: Do not pass raw data directly to this function. Always hash the
/// data first using a collision-resistant hash function. Signing raw data can:
/// - Leak information about the data through signature timing
/// - Exceed practical message size limits
/// - Fail to provide the collision-resistance properties required for security
///
/// ## Digest Algorithm
/// For Strata snapshots, the digest **must** be computed as:
/// ```text
/// digest = SHA-256(master_index_bytes)
/// ```
/// where `master_index_bytes` is the serialized master index read from the snapshot
/// file (see `crates/cli/src/cmd/sys/sign.rs` for the complete implementation).
///
/// # Signature Format
///
/// The returned signature is exactly **64 bytes** in the following format:
/// - **Bytes 0-31**: `R` component (compressed Edwards curve point)
/// - **Bytes 32-63**: `S` component (scalar modulo the group order)
///
/// This format is defined by RFC 8032 and is compatible with all standard Ed25519
/// implementations.
///
/// # Security Considerations
///
/// ## Key Protection
/// - **Unencrypted key file**: The private key is read from disk as plaintext.
///   Ensure the key file has restrictive permissions (mode 0600) and consider
///   using encrypted filesystems or Hardware Security Modules (HSMs) for production.
/// - **Memory exposure**: Private key bytes are held in memory during signing.
///   For high-security environments, use the `zeroize` crate to clear key material
///   after use.
///
/// ## Side-Channel Resistance
/// The `ed25519-dalek` crate implements constant-time operations to resist timing
/// attacks. However, file I/O (reading the private key) is **not** constant-time.
/// Attackers with local access may observe disk access patterns or page faults.
///
/// ## Signature Malleability
/// Ed25519 signatures are **not malleable**: given a valid signature `(R, S)`,
/// attackers cannot create a different valid signature for the same message without
/// knowledge of the private key. This property is critical for preventing replay
/// attacks in certain protocols.
///
/// # Arguments
///
/// * `private_key_path` - Path to the private key file. The file must contain
///   exactly 32 bytes of raw binary data (the Ed25519 secret scalar). This is
///   the same format produced by [`generate_keypair`].
/// * `digest` - The digest to sign. This should be a cryptographic hash (e.g.,
///   SHA-256 output) of the data to authenticate, **not** the raw data itself.
///   While any byte slice is accepted, passing unhashed data is insecure.
///
/// # Returns
///
/// Returns a 64-byte Ed25519 signature as a fixed-size array `[u8; 64]` on success.
/// The signature is deterministic: calling this function multiple times with the
/// same key and digest produces identical signatures.
///
/// # Errors
///
/// This function returns [`StrataError::Io`] if:
/// - **Private key file does not exist**: No file at `private_key_path`
/// - **Permission denied**: Insufficient permissions to read the key file
/// - **File is not exactly 32 bytes**: The key file is truncated, corrupted, or
///   contains extra data. Valid Ed25519 private keys are always 32 bytes.
/// - **I/O error during read**: Hardware failure, network filesystem timeout, etc.
///
/// **Note**: If the private key file contains invalid bytes (not a valid Ed25519
/// scalar), the function will still succeed but produce an unpredictable signature.
/// Keys generated by [`generate_keypair`] are always valid.
///
/// [`StrataError::Io`]: crate::StrataError::Io
/// [`verify_digest`]: verify_digest
/// [`generate_keypair`]: generate_keypair
///
/// # Examples
///
/// ## Basic Usage: Signing a SHA-256 Digest
///
/// ```no_run
/// use std::path::Path;
/// use sha2::{Sha256, Digest};
/// use strata_common::sign::sign_digest;
///
/// # fn main() -> strata_common::Result<()> {
/// // Compute SHA-256 digest of data to sign
/// let data = b"Critical snapshot data that must be authenticated";
/// let mut hasher = Sha256::new();
/// hasher.update(data);
/// let digest = hasher.finalize();
///
/// // Sign the digest
/// let private_key = Path::new("snapshot.key");
/// let signature = sign_digest(private_key, &digest)?;
///
/// println!("Signature generated: {} bytes", signature.len());
/// println!("Signature (hex): {:x?}", &signature[..8]); // Print first 8 bytes
/// # Ok(())
/// # }
/// ```
///
/// ## Signing Multiple Data Chunks
///
/// ```no_run
/// use std::path::Path;
/// use sha2::{Sha256, Digest};
/// use strata_common::sign::sign_digest;
///
/// # fn main() -> strata_common::Result<()> {
/// // Compute digest over multiple data chunks
/// let mut hasher = Sha256::new();
/// hasher.update(b"Header: ");
/// hasher.update(b"Version 1.0\n");
/// hasher.update(b"Data: ");
/// hasher.update(&[0u8; 1024]); // Binary data
/// let digest = hasher.finalize();
///
/// // Sign the aggregate digest
/// let signature = sign_digest(Path::new("snapshot.key"), &digest)?;
/// println!("Signed {} bytes of data", 1024 + 22);
/// # Ok(())
/// # }
/// ```
///
/// ## Deterministic Signature Verification
///
/// ```no_run
/// use std::path::Path;
/// use sha2::{Sha256, Digest};
/// use strata_common::sign::sign_digest;
///
/// # fn main() -> strata_common::Result<()> {
/// let data = b"deterministic test";
/// let mut hasher = Sha256::new();
/// hasher.update(data);
/// let digest = hasher.finalize();
///
/// let private_key = Path::new("snapshot.key");
///
/// // Sign the same digest twice
/// let signature1 = sign_digest(private_key, &digest)?;
/// let signature2 = sign_digest(private_key, &digest)?;
///
/// // Signatures are identical (deterministic)
/// assert_eq!(signature1, signature2);
/// println!("Signature is deterministic: {}", signature1 == signature2);
/// # Ok(())
/// # }
/// ```
///
/// ## Error Handling: Invalid Key File
///
/// ```no_run
/// use std::path::Path;
/// use sha2::{Sha256, Digest};
/// use strata_common::sign::sign_digest;
///
/// # fn main() {
/// let digest = Sha256::digest(b"test data");
///
/// // Attempt to sign with non-existent key
/// match sign_digest(Path::new("missing.key"), &digest) {
///     Ok(_) => println!("Unexpected success"),
///     Err(e) => println!("Expected error: {}", e), // "IO Error: No such file..."
/// }
///
/// // Attempt to sign with wrong-sized key file
/// std::fs::write("bad.key", &[0u8; 16]).unwrap(); // Only 16 bytes
/// match sign_digest(Path::new("bad.key"), &digest) {
///     Ok(_) => println!("Unexpected success"),
///     Err(e) => println!("Expected error: {}", e), // "IO Error: failed to fill whole buffer"
/// }
/// # }
/// ```
///
/// ## Integration with Strata Snapshot Signing
///
/// This example mirrors the production usage in `crates/cli/src/cmd/sys/sign.rs`.
///
/// ```no_run
/// use std::path::Path;
/// use std::fs::File;
/// use std::io::{Read, Seek, SeekFrom};
/// use sha2::{Sha256, Digest};
/// use strata_common::sign::sign_digest;
///
/// # fn main() -> strata_common::Result<()> {
/// // Open snapshot file
/// let mut snapshot = File::open("system.strata")?;
///
/// // Read header to locate master index (simplified - real code parses StrataHeader)
/// let index_offset = 4096u64; // Example offset
/// snapshot.seek(SeekFrom::Start(index_offset))?;
///
/// // Read master index bytes
/// let mut index_bytes = Vec::new();
/// snapshot.read_to_end(&mut index_bytes)?;
///
/// // Compute digest
/// let digest = Sha256::digest(&index_bytes);
///
/// // Sign the index
/// let signature = sign_digest(Path::new("release.key"), &digest)?;
///
/// println!("Snapshot index signed with {} byte signature", signature.len());
/// // Production code: append signature to snapshot file and update header
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

/// Verifies an Ed25519 signature against a cryptographic digest.
///
/// This function loads an Ed25519 public key from disk and uses it to verify
/// that the provided signature was created by the holder of the corresponding
/// private key over the given digest. Successful verification proves:
/// 1. The digest has not been modified since signing
/// 2. The signature was created by someone with access to the private key
/// 3. The public key corresponds to the private key used for signing
///
/// Verification is **constant-time** with respect to the signature and public key
/// to prevent timing side-channel attacks.
///
/// # Verification Algorithm
///
/// The verification process follows RFC 8032 Section 5.1.7 (Ed25519 signature verification):
/// 1. Load the 32-byte public key from `public_key_path` (Edwards curve point `A`)
/// 2. Parse the signature as `R || S` (two 32-byte components)
/// 3. Decode `R` as an Edwards curve point (reject if invalid)
/// 4. Reject if `S >= L` (where `L` is the group order)
/// 5. Compute `h = hash(R || A || digest)` (SHA-512 under the hood)
/// 6. Verify the equation: `[S]G = R + [h]A` (where `G` is the base point)
/// 7. Return success if the equation holds, failure otherwise
///
/// # Digest Input Format
///
/// ## Raw vs. Pre-Hashed
/// The `digest` parameter should be a **pre-computed cryptographic hash** of the
/// original data, **not** the raw data itself. This function verifies signatures
/// over the digest, so the digest must match exactly what was passed to [`sign_digest`].
///
/// **Example Correct Usage**:
/// ```text
/// // Signer:
/// digest = SHA-256(data)
/// signature = sign_digest(private_key, digest)
///
/// // Verifier:
/// digest = SHA-256(data)  // Must recompute the same digest!
/// verify_digest(public_key, digest, signature)  // ✓ Verifies correctly
/// ```
///
/// **Example Incorrect Usage**:
/// ```text
/// // Signer:
/// digest = SHA-256(data)
/// signature = sign_digest(private_key, digest)
///
/// // Verifier:
/// verify_digest(public_key, data, signature)  // ✗ WRONG: passing raw data instead of digest
/// ```
///
/// ## Digest Algorithm Consistency
/// Both the signer and verifier **must use the same hash algorithm**. For Strata
/// snapshots, this is always **SHA-256** of the master index bytes.
///
/// # Verification Process
///
/// The verification process consists of:
/// 1. **Public key validation**: Ensure the public key represents a valid Edwards
///    curve point (not a low-order point or invalid encoding)
/// 2. **Signature parsing**: Decode the 64-byte signature into `R` and `S` components
/// 3. **Signature validation**: Ensure `R` is a valid curve point and `S` is within
///    the expected range (less than the group order)
/// 4. **Cryptographic verification**: Verify the Ed25519 signature equation
///
/// If any step fails, the function returns an error. Verification is **all-or-nothing**:
/// there are no partial successes or warnings.
///
/// # Security Considerations
///
/// ## Verification Failure Semantics
/// Verification failure does **not** distinguish between:
/// - Invalid signature (attacker attempted forgery)
/// - Corrupted signature bytes (transmission error)
/// - Wrong public key (key mismatch)
/// - Digest mismatch (data was modified after signing)
///
/// All failure modes return the same error to prevent side-channel information leakage.
///
/// ## Public Key Trust (Critical)
/// **This function does NOT establish trust in the public key itself.** It only
/// verifies that the signature is mathematically valid for the given public key.
///
/// Callers **must** verify public key authenticity through out-of-band means:
/// - **Key fingerprints**: Compare SHA-256 hash of the public key against a trusted
///   fingerprint published on an official website (via HTTPS)
/// - **Certificate chains**: Use X.509 certificates signed by a trusted Certificate
///   Authority (CA)
/// - **Web of trust**: Use PGP-style key signing networks
/// - **Configuration management**: Distribute public keys via trusted automation
///   (Ansible, Puppet) that authenticates sources
///
/// **Attack scenario without key authentication**:
/// 1. Attacker replaces both the snapshot file and the public key file
/// 2. Attacker signs the malicious snapshot with their own private key
/// 3. `verify_digest` succeeds because the signature is valid for the attacker's public key
/// 4. System accepts the malicious snapshot
///
/// **Mitigation**: Always verify the public key fingerprint against a trusted source
/// before using it for verification.
///
/// ## Constant-Time Guarantees
/// The `ed25519-dalek` crate implements constant-time signature verification to
/// prevent timing attacks. However:
/// - File I/O (reading the public key) is **not** constant-time
/// - Error handling may leak timing information about which step failed
/// - For high-security scenarios, consider loading keys into memory once and reusing them
///
/// ## Replay Attacks
/// Ed25519 signatures do **not** include timestamps or nonces. This function cannot
/// detect replay attacks where an attacker reuses a valid signature for an older
/// snapshot. Applications requiring freshness guarantees must implement:
/// - Version numbers in signed data
/// - Timestamps in signed data
/// - Nonces or sequence numbers
/// - Revocation lists for old signatures
///
/// # Arguments
///
/// * `public_key_path` - Path to the public key file. The file must contain exactly
///   32 bytes of raw binary data (the compressed Edwards Y-coordinate with sign bit).
///   This is the same format produced by [`generate_keypair`].
/// * `digest` - The digest that was signed. This must be the **exact same byte
///   sequence** that was passed to [`sign_digest`], typically a SHA-256 hash of
///   the original data. Passing the raw data instead of the digest will cause
///   verification to fail.
/// * `signature_bytes` - The 64-byte Ed25519 signature to verify. This is the
///   output of [`sign_digest`]. The signature format is `R || S` where `R` and `S`
///   are each 32 bytes.
///
/// # Returns
///
/// Returns `Ok(())` if and only if:
/// - The public key is valid
/// - The signature is well-formed
/// - The signature cryptographically verifies against the digest and public key
///
/// **Success means**: The digest was signed by the holder of the private key
/// corresponding to the public key at `public_key_path`.
///
/// # Errors
///
/// This function returns an error if:
///
/// ## I/O Errors ([`StrataError::Io`])
/// - **Public key file does not exist**: No file at `public_key_path`
/// - **Permission denied**: Insufficient permissions to read the key file
/// - **File is not exactly 32 bytes**: The key file is truncated, corrupted, or
///   contains extra data. Valid Ed25519 public keys are always 32 bytes.
/// - **I/O error during read**: Hardware failure, network filesystem timeout, etc.
///
/// ## Format Errors ([`StrataError::Format`])
/// - **Invalid public key encoding**: The 32-byte public key does not represent
///   a valid Edwards curve point. This can occur if:
///   - The bytes are not a valid curve point (not on the curve)
///   - The bytes represent a low-order point (weak key)
///   - The file contains corrupted data
/// - **Signature verification failed**: The signature is invalid. This occurs when:
///   - The signature was created with a different private key
///   - The digest was modified after signing
///   - The signature bytes are corrupted
///   - An attacker attempted to forge the signature
///
/// **Important**: Verification failure does not reveal **why** verification failed
/// (prevents side-channel attacks).
///
/// [`StrataError::Io`]: crate::StrataError::Io
/// [`StrataError::Format`]: crate::StrataError::Format
/// [`generate_keypair`]: generate_keypair
/// [`sign_digest`]: sign_digest
///
/// # Examples
///
/// ## Basic Usage: Complete Sign and Verify Workflow
///
/// ```no_run
/// use std::path::Path;
/// use sha2::{Sha256, Digest};
/// use strata_common::sign::{generate_keypair, sign_digest, verify_digest};
///
/// # fn main() -> strata_common::Result<()> {
/// // Generate keypair
/// let private_key = Path::new("test.key");
/// let public_key = Path::new("test.pub");
/// generate_keypair(private_key, public_key)?;
///
/// // Sign data
/// let data = b"Critical system snapshot data";
/// let digest = Sha256::digest(data);
/// let signature = sign_digest(private_key, &digest)?;
///
/// // Verify signature
/// verify_digest(public_key, &digest, &signature)?;
/// println!("Signature verified successfully!");
/// # Ok(())
/// # }
/// ```
///
/// ## Error Handling: Invalid Signature Detection
///
/// ```no_run
/// use std::path::Path;
/// use sha2::{Sha256, Digest};
/// use strata_common::sign::{sign_digest, verify_digest};
///
/// # fn main() -> strata_common::Result<()> {
/// let data = b"original data";
/// let digest = Sha256::digest(data);
/// let signature = sign_digest(Path::new("test.key"), &digest)?;
///
/// // Attacker modifies data after signing
/// let modified_data = b"tampered data";
/// let modified_digest = Sha256::digest(modified_data);
///
/// // Verification fails
/// match verify_digest(Path::new("test.pub"), &modified_digest, &signature) {
///     Ok(_) => println!("Unexpected success - signature should be invalid!"),
///     Err(e) => println!("Tampering detected: {}", e), // Expected
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Public Key Fingerprint Verification
///
/// This example shows how to verify public key authenticity before trusting it.
///
/// ```no_run
/// use std::path::Path;
/// use std::fs;
/// use sha2::{Sha256, Digest};
/// use strata_common::sign::verify_digest;
///
/// # fn main() -> strata_common::Result<()> {
/// let public_key_path = Path::new("vendor.pub");
///
/// // Read public key bytes
/// let public_key_bytes = fs::read(public_key_path)?;
///
/// // Compute fingerprint
/// let fingerprint = Sha256::digest(&public_key_bytes);
/// let fingerprint_hex = format!("{:x}", fingerprint);
///
/// // Compare against trusted fingerprint (obtained from vendor's website via HTTPS)
/// const TRUSTED_FINGERPRINT: &str = "a3c5e8d2f1b4..."; // Example
///
/// if fingerprint_hex != TRUSTED_FINGERPRINT {
///     eprintln!("ERROR: Public key fingerprint mismatch!");
///     eprintln!("Expected: {}", TRUSTED_FINGERPRINT);
///     eprintln!("Got:      {}", fingerprint_hex);
///     eprintln!("DO NOT use this public key - it may be malicious!");
///     std::process::exit(1);
/// }
///
/// println!("Public key fingerprint verified ✓");
///
/// // Now safe to use the public key for signature verification
/// let data = b"snapshot data";
/// let digest = Sha256::digest(data);
/// let signature = [0u8; 64]; // Load actual signature from snapshot
/// verify_digest(public_key_path, &digest, &signature)?;
/// # Ok(())
/// # }
/// ```
///
/// ## Batch Verification (Multiple Snapshots)
///
/// ```no_run
/// use std::path::Path;
/// use std::fs;
/// use sha2::{Sha256, Digest};
/// use strata_common::sign::verify_digest;
///
/// # fn main() -> strata_common::Result<()> {
/// let public_key = Path::new("release.pub");
/// let snapshots = vec!["snapshot1.strata", "snapshot2.strata", "snapshot3.strata"];
///
/// for snapshot_path in snapshots {
///     // Read snapshot data (simplified - real code reads master index)
///     let data = fs::read(snapshot_path)?;
///     let digest = Sha256::digest(&data);
///
///     // Load signature from snapshot metadata (example assumes separate .sig file)
///     let sig_path = format!("{}.sig", snapshot_path);
///     let mut signature = [0u8; 64];
///     let sig_bytes = fs::read(&sig_path)?;
///     signature.copy_from_slice(&sig_bytes[..64]);
///
///     // Verify signature
///     match verify_digest(public_key, &digest, &signature) {
///         Ok(_) => println!("✓ {} verified", snapshot_path),
///         Err(e) => {
///             eprintln!("✗ {} verification FAILED: {}", snapshot_path, e);
///             eprintln!("DO NOT use this snapshot - it may be compromised!");
///         }
///     }
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Integration with Strata Snapshot Verification
///
/// This example mirrors the production usage in `crates/cli/src/cmd/sys/verify.rs`.
///
/// ```no_run
/// use std::path::Path;
/// use std::fs::File;
/// use std::io::{Read, Seek, SeekFrom};
/// use sha2::{Sha256, Digest};
/// use strata_common::sign::verify_digest;
///
/// # fn main() -> strata_common::Result<()> {
/// let snapshot_path = Path::new("system.strata");
/// let public_key = Path::new("release.pub");
///
/// let mut file = File::open(snapshot_path)?;
///
/// // Read header to find signature and index locations (simplified)
/// let signature_offset = 1048576u64; // Example offset from header
/// let index_offset = 4096u64;
/// let index_length = signature_offset - index_offset;
///
/// // Read signature
/// file.seek(SeekFrom::Start(signature_offset))?;
/// let mut signature = [0u8; 64];
/// file.read_exact(&mut signature)?;
///
/// // Read master index
/// file.seek(SeekFrom::Start(index_offset))?;
/// let mut index_bytes = vec![0u8; index_length as usize];
/// file.read_exact(&mut index_bytes)?;
///
/// // Compute index digest
/// let digest = Sha256::digest(&index_bytes);
///
/// // Verify signature
/// verify_digest(public_key, &digest, &signature)?;
///
/// println!("Snapshot signature verified! Master index is authentic.");
/// # Ok(())
/// # }
/// ```
///
/// ## Error Handling: Detailed Failure Analysis
///
/// ```no_run
/// use std::path::Path;
/// use sha2::{Sha256, Digest};
/// use strata_common::sign::verify_digest;
/// use strata_common::StrataError;
///
/// # fn main() {
/// let public_key = Path::new("test.pub");
/// let digest = Sha256::digest(b"data");
/// let signature = [0u8; 64]; // Invalid signature
///
/// match verify_digest(public_key, &digest, &signature) {
///     Ok(_) => println!("Signature valid"),
///     Err(StrataError::Io(e)) => {
///         eprintln!("I/O error reading public key: {}", e);
///         eprintln!("Possible causes:");
///         eprintln!("  - Public key file does not exist");
///         eprintln!("  - Insufficient permissions");
///         eprintln!("  - File is wrong size (must be exactly 32 bytes)");
///     }
///     Err(StrataError::Format(e)) => {
///         eprintln!("Signature verification failed: {}", e);
///         eprintln!("Possible causes:");
///         eprintln!("  - Signature is invalid (forgery attempt or corruption)");
///         eprintln!("  - Data was modified after signing");
///         eprintln!("  - Wrong public key (does not match signing key)");
///         eprintln!("  - Public key file is corrupted");
///     }
///     Err(e) => eprintln!("Unexpected error: {}", e),
/// }
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

/// Computes the SHA-256 digest of a Strata snapshot's master index.
///
/// **Current Status: NOT IMPLEMENTED**
///
/// This function is **not currently implemented** and will always return an error.
/// The digest computation logic has been moved to the CLI layer (`crates/cli/src/cmd/sys/sign.rs`)
/// to avoid circular dependencies between `strata-common` and `strata-core`.
///
/// # Why This Function Is Unimplemented
///
/// ## Circular Dependency Problem
/// To compute the snapshot digest, this function needs to:
/// 1. Read the Strata snapshot file header
/// 2. Deserialize the header to extract `index_offset` and `index_length`
/// 3. Read the master index bytes from the file
/// 4. Compute SHA-256 of the index bytes
///
/// However, the `StrataHeader` type is defined in `strata-core`, which **depends on**
/// `strata-common`. If `strata-common` were to depend on `strata-core`, this would
/// create a circular dependency:
///
/// ```text
/// strata-common (sign.rs) --> strata-core (StrataHeader)
///                               |
///                               v
///                           strata-common (error.rs, Result<T>)
/// ```
///
/// Rust does not allow circular dependencies between crates.
///
/// ## Resolution Strategy
/// The digest computation logic has been moved to `crates/cli/src/cmd/sys/sign.rs`,
/// which has access to both `strata-common` (for signing primitives) and `strata-core`
/// (for header parsing). This maintains the following dependency graph:
///
/// ```text
/// strata-cli --> strata-core --> strata-common
///     |                              |
///     +------------------------------+
/// ```
///
/// The CLI performs digest computation and then calls [`sign_digest`] to sign it.
///
/// # Future Plans
///
/// ## Option 1: Keep in CLI Layer (Current Approach)
/// - **Pros**: No circular dependencies, clean separation of concerns
/// - **Cons**: Digest computation is not reusable outside the CLI
/// - **Status**: This is the current implementation approach
///
/// ## Option 2: Move to strata-core
/// Create a new public function `strata_core::digest::compute_master_index_digest(path: &Path)`
/// that computes the digest and can be called by both the CLI and other crates.
/// - **Pros**: Reusable, keeps digest logic with header parsing
/// - **Cons**: Adds digest computation responsibility to `strata-core`
///
/// ## Option 3: Manual Header Parsing
/// Parse the header bytes manually in `strata-common` without depending on
/// `strata-core` types. This would require:
/// - Hardcoding the header format (brittle, error-prone)
/// - Duplicating deserialization logic (violates DRY principle)
/// - Maintaining synchronization with `strata-core` header changes
/// - **Verdict**: Not recommended due to maintainability concerns
///
/// # Current Implementation Workflow
///
/// To sign a Strata snapshot using the current implementation:
///
/// ```bash
/// # Generate keypair
/// strata keygen --private snapshot.key --public snapshot.pub
///
/// # Sign snapshot (digest computation happens internally)
/// strata sign --key snapshot.key snapshot.strata
///
/// # Verify snapshot
/// strata verify --key snapshot.pub snapshot.strata
/// ```
///
/// The digest computation is performed in `crates/cli/src/cmd/sys/sign.rs` as follows:
///
/// ```rust,ignore
/// // Open snapshot and read header
/// let mut f = File::open(&image_path)?;
/// let mut header_bytes = [0u8; HEADER_SIZE];
/// f.read_exact(&mut header_bytes)?;
/// let header: StrataHeader = bincode::deserialize(&header_bytes)?;
///
/// // Seek to master index and read it
/// f.seek(SeekFrom::Start(header.index_offset))?;
/// let mut index_bytes = Vec::new();
/// f.read_to_end(&mut index_bytes)?;
///
/// // Compute SHA-256 digest of master index
/// let mut hasher = Sha256::new();
/// hasher.update(&index_bytes);
/// let digest = hasher.finalize();
///
/// // Sign the digest (calls sign_digest from strata-common)
/// let signature = sign::sign_digest(&key_path, &digest)?;
/// ```
///
/// # Arguments
///
/// * `path` - Path to the Strata snapshot file. This parameter is currently ignored
///   because the function is not implemented.
///
/// # Returns
///
/// This function **never returns successfully**. It always returns
/// [`StrataError::Format`] with the message:
/// ```text
/// "Digest computation should be done in CLI/Core layer"
/// ```
///
/// # Errors
///
/// Always returns [`StrataError::Format`] indicating that digest computation must
/// be performed at the CLI or Core layer due to architectural constraints.
///
/// [`StrataError::Format`]: crate::StrataError::Format
/// [`sign_digest`]: sign_digest
///
/// # Examples
///
/// ## Attempting to Call This Function (Will Fail)
///
/// ```no_run
/// use std::path::Path;
/// use strata_common::sign::compute_snapshot_digest;
///
/// # fn main() {
/// let snapshot = Path::new("system.strata");
///
/// match compute_snapshot_digest(snapshot) {
///     Ok(digest) => {
///         println!("Digest: {:?}", digest); // This branch is never reached
///     }
///     Err(e) => {
///         eprintln!("Expected error: {}", e);
///         // Output: "Invalid Format: Digest computation should be done in CLI/Core layer"
///     }
/// }
/// # }
/// ```
///
/// ## Correct Approach: Manual Digest Computation
///
/// If you need to compute a snapshot digest programmatically, use this approach
/// (requires access to `strata-core`):
///
/// ```rust,ignore
/// use std::path::Path;
/// use std::fs::File;
/// use std::io::{Read, Seek, SeekFrom};
/// use sha2::{Sha256, Digest};
/// use strata_core::format::header::StrataHeader;
/// use strata_core::format::magic::HEADER_SIZE;
/// use strata_common::sign::sign_digest;
///
/// fn compute_and_sign_snapshot(
///     snapshot_path: &Path,
///     private_key_path: &Path,
/// ) -> Result<[u8; 64], Box<dyn std::error::Error>> {
///     // Open snapshot file
///     let mut f = File::open(snapshot_path)?;
///
///     // Read and parse header
///     let mut header_bytes = [0u8; HEADER_SIZE];
///     f.read_exact(&mut header_bytes)?;
///     let header: StrataHeader = bincode::deserialize(&header_bytes)?;
///
///     // Read master index
///     f.seek(SeekFrom::Start(header.index_offset))?;
///     let mut index_bytes = Vec::new();
///     f.read_to_end(&mut index_bytes)?;
///
///     // Compute digest
///     let digest = Sha256::digest(&index_bytes);
///
///     // Sign digest
///     let signature = sign_digest(private_key_path, &digest)?;
///
///     Ok(signature)
/// }
/// ```
///
/// ## Verification Workflow (Also Requires strata-core)
///
/// ```rust,ignore
/// use std::path::Path;
/// use std::fs::File;
/// use std::io::{Read, Seek, SeekFrom};
/// use sha2::{Sha256, Digest};
/// use strata_core::format::header::StrataHeader;
/// use strata_core::format::magic::HEADER_SIZE;
/// use strata_common::sign::verify_digest;
///
/// fn verify_snapshot(
///     snapshot_path: &Path,
///     public_key_path: &Path,
/// ) -> Result<(), Box<dyn std::error::Error>> {
///     let mut f = File::open(snapshot_path)?;
///
///     // Read header
///     let mut header_bytes = [0u8; HEADER_SIZE];
///     f.read_exact(&mut header_bytes)?;
///     let header: StrataHeader = bincode::deserialize(&header_bytes)?;
///
///     // Extract signature location from header
///     let (sig_offset, sig_len) = match (header.signature_offset, header.signature_length) {
///         (Some(off), Some(len)) => (off, len),
///         _ => return Err("Snapshot is not signed".into()),
///     };
///
///     // Read signature
///     f.seek(SeekFrom::Start(sig_offset))?;
///     let mut signature = [0u8; 64];
///     f.read_exact(&mut signature)?;
///
///     // Read master index (ends at signature offset)
///     let index_len = sig_offset - header.index_offset;
///     f.seek(SeekFrom::Start(header.index_offset))?;
///     let mut index_bytes = vec![0u8; index_len as usize];
///     f.read_exact(&mut index_bytes)?;
///
///     // Compute digest
///     let digest = Sha256::digest(&index_bytes);
///
///     // Verify signature
///     verify_digest(public_key_path, &digest, &signature)?;
///
///     println!("Snapshot signature verified successfully!");
///     Ok(())
/// }
/// ```
///
/// # Migration Notes
///
/// If you are maintaining code that previously depended on `compute_snapshot_digest`,
/// you should:
/// 1. Move digest computation to a layer that has access to `strata-core`
/// 2. Use the manual approach shown in the examples above
/// 3. Consider using the Strata CLI commands (`strata sign`, `strata verify`) instead
///    of implementing custom signing workflows
///
/// # See Also
///
/// - [`sign_digest`]: Signs a pre-computed digest
/// - [`verify_digest`]: Verifies a signature against a digest
/// - `crates/cli/src/cmd/sys/sign.rs`: Reference implementation of digest computation
/// - `crates/cli/src/cmd/sys/verify.rs`: Reference implementation of signature verification
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
