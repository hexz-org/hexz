//! Verify Ed25519 signatures on Hexz archives.
//!
//! This module implements the `verify` command, which validates the cryptographic
//! signature on a signed Hexz archive to ensure authenticity and integrity.
//!
//! # Verification Process
//!
//! The verification operation follows these steps:
//!
//! 1. **Load Header**: Read archive header and extract signature metadata
//! 2. **Check Signature Exists**: Verify the archive has been signed
//! 3. **Read Signature**: Load the 64-byte Ed25519 signature from the archive
//! 4. **Read Master Index**: Read the index structure that was signed
//! 5. **Compute Digest**: Calculate SHA-256 hash of the index
//! 6. **Verify Signature**: Validate Ed25519 signature using public key
//!
//! # What Gets Verified
//!
//! The signature verification checks:
//! - The **Master Index** has not been modified since signing
//! - The signature was created by the holder of the corresponding private key
//! - The signature is mathematically valid (correct Ed25519 signature)
//!
//! # Security Guarantees
//!
//! A valid signature proves:
//! - **Authenticity**: Archive was signed by holder of the private key
//! - **Integrity**: Index structure has not been tampered with
//! - **Trust**: If you trust the public key, you can trust the archive
//!
//! # Limitations
//!
//! Signature verification does NOT protect against:
//! - **Replay attacks**: Old valid archives can be replayed
//! - **Data block modification**: Individual blocks could be swapped if hashes collide
//! - **Header manipulation**: Some header fields are mutable (e.g., signature metadata)
//!
//! # Usage
//!
//! ```bash
//! # Verify an archive signature
//! hexz sys verify --key ~/.hexz/keys/public.key archive.st
//!
//! # On success
//! # => Signature Verified! The image index is authentic.
//!
//! # On failure
//! # => Error: Signature verification failed
//! ```
//!
//! # Exit Codes
//!
//! - **0**: Signature is valid
//! - **Non-zero**: Verification failed (invalid signature or archive not signed)

use anyhow::Result;
use hexz_ops::sign::verify_archive;
use std::path::PathBuf;

/// Verify the Ed25519 signature on a signed Hexz archive.
///
/// This function validates that the archive's Master Index has not been modified
/// since it was signed, and that the signature was created by the holder of the
/// corresponding private key.
///
/// # Arguments
///
/// * `key_path` - Path to the Ed25519 public key file (32 bytes)
/// * `image_path` - Path to the signed Hexz archive file
///
/// # Process
///
/// 1. Opens the archive and reads the header
/// 2. Checks that signature metadata exists in header
/// 3. Reads the 64-byte signature from the file
/// 4. Reads the Master Index (from header.index_offset to signature offset)
/// 5. Computes SHA-256 digest of the index
/// 6. Verifies the Ed25519 signature against the digest
///
/// # Returns
///
/// Returns `Ok(())` if signature is valid, or an error if:
/// - Archive is not signed (missing signature metadata)
/// - Public key file cannot be read
/// - Archive file is malformed
/// - Signature length is invalid (not 64 bytes)
/// - Signature verification fails (tampered index or wrong key)
///
/// # Example
///
/// ```no_run
/// # use std::path::PathBuf;
/// # use hexz_cli::cmd::sys::verify;
/// let key = PathBuf::from("~/.hexz/keys/public.key");
/// let archive = PathBuf::from("archive.hxz");
///
/// match verify::run(key, archive) {
///     Ok(()) => println!("✓ Signature valid"),
///     Err(e) => eprintln!("✗ Verification failed: {}", e),
/// }
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run(key_path: PathBuf, image_path: PathBuf) -> Result<()> {
    use colored::*;
    println!("{} Verifying archive", "╭".dimmed());
    println!("{} Image     {}", "│".dimmed(), image_path.display().to_string().cyan());
    println!("{} Key       {}", "╰".dimmed(), key_path.display().to_string().bright_black());

    verify_archive(&image_path, &key_path)?;
    println!("\n  {} Signature verified. The index is authentic.", "✓".green());
    Ok(())
}
