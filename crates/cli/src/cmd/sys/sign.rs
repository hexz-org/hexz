//! Cryptographically sign Hexz archives with Ed25519 signatures.
//!
//! This module implements the `sign` command, which creates a cryptographic
//! signature for a Hexz archive to ensure authenticity and integrity.
//!
//! # Signing Process
//!
//! The signing operation follows these steps:
//!
//! 1. **Load Header**: Read and parse the archive header
//! 2. **Read Master Index**: Read the entire index structure (block mappings)
//! 3. **Compute Digest**: Calculate SHA-256 hash of the index
//! 4. **Sign Digest**: Create Ed25519 signature using private key
//! 5. **Append Signature**: Write 64-byte signature to end of file
//! 6. **Update Header**: Record signature offset and length in header
//!
//! # What Gets Signed
//!
//! The signature covers the **Master Index** only, not the entire file. This is
//! because:
//! - The header is mutable (to store signature metadata)
//! - Data blocks are content-addressed via their hashes in the index
//! - Signing the index ensures block mappings haven't been tampered with
//!
//! # Signature Format
//!
//! - **Algorithm**: Ed25519 (EdDSA on Curve25519)
//! - **Digest**: SHA-256 of Master Index
//! - **Signature Size**: 64 bytes
//! - **Storage**: Appended to end of archive file
//!
//! # Security Properties
//!
//! - **Authenticity**: Proves the archive was created by holder of private key
//! - **Integrity**: Detects any modification to the index structure
//! - **Non-repudiation**: Signer cannot deny creating the signature
//!
//! # Usage
//!
//! ```bash
//! # Generate keys first
//! hexz sys keygen --output-dir ~/.hexz/keys
//!
//! # Sign an archive
//! hexz sys sign --key ~/.hexz/keys/private.key archive.st
//!
//! # Verify the signature
//! hexz sys verify --key ~/.hexz/keys/public.key archive.st
//! ```
//!
//! # File Format Changes
//!
//! After signing, the archive structure becomes:
//!
//! ```text
//! ┌─────────────────┐
//! │  Header         │ signature_offset, signature_length fields updated
//! ├─────────────────┤
//! │  Index          │ ← This is what gets signed (SHA-256 digest)
//! ├─────────────────┤
//! │  Data Blocks    │
//! ├─────────────────┤
//! │  Signature      │ ← 64-byte Ed25519 signature (appended)
//! └─────────────────┘
//! ```

use anyhow::Result;
use hexz_ops::sign::sign_archive;
use std::path::PathBuf;

/// Sign a Hexz archive with an Ed25519 private key.
///
/// This function creates a cryptographic signature for the archive's Master Index
///and embeds it in the archive file, updating the header to record the signature's
/// location.
///
/// # Arguments
///
/// * `key_path` - Path to the Ed25519 private key file (32 bytes)
/// * `image_path` - Path to the Hexz archive file to sign
///
/// # Process
///
/// 1. Opens the archive and reads the header
/// 2. Reads the entire Master Index structure
/// 3. Computes SHA-256 digest of the index
/// 4. Signs the digest with Ed25519 private key
/// 5. Appends 64-byte signature to end of file
/// 6. Updates header with signature offset/length
///
/// # Returns
///
/// Returns `Ok(())` on success, or an error if:
/// - Private key file cannot be read
/// - Archive file cannot be opened or is malformed
/// - Header cannot be parsed
/// - Signature generation fails
/// - File I/O errors occur
///
/// # Side Effects
///
/// - Modifies the archive file (appends signature, updates header)
/// - Existing signature (if any) is replaced
///
/// # Example
///
/// ```no_run
/// # use std::path::PathBuf;
/// # use hexz_cli::cmd::sys::sign;
/// let key = PathBuf::from("~/.hexz/keys/private.key");
/// let archive = PathBuf::from("archive.hxz");
/// sign::run(key, archive)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run(key_path: PathBuf, image_path: PathBuf) -> Result<()> {
    use colored::*;
    println!("{} Signing archive", "╭".dimmed());
    println!("{} Image     {}", "│".dimmed(), image_path.display().to_string().cyan());
    println!("{} Key       {}", "╰".dimmed(), key_path.display().to_string().bright_black());

    sign_archive(&image_path, &key_path)?;
    println!("\n  {} Signature written successfully.", "✓".green());
    Ok(())
}
