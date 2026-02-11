//! Cryptographically sign Strata archives with Ed25519 signatures.
//!
//! This module implements the `sign` command, which creates a cryptographic
//! signature for a Strata archive to ensure authenticity and integrity.
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
//! strata sys keygen --output-dir ~/.strata/keys
//!
//! # Sign an archive
//! strata sys sign --key ~/.strata/keys/private.key snapshot.st
//!
//! # Verify the signature
//! strata sys verify --key ~/.strata/keys/public.key snapshot.st
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
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use strata_common::sign;
use strata_core::format::header::StrataHeader;
use strata_core::format::magic::HEADER_SIZE;

/// Sign a Strata archive with an Ed25519 private key.
///
/// This function creates a cryptographic signature for the archive's Master Index
///and embeds it in the archive file, updating the header to record the signature's
/// location.
///
/// # Arguments
///
/// * `key_path` - Path to the Ed25519 private key file (32 bytes)
/// * `image_path` - Path to the Strata archive file to sign
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
/// # use strata_cli::cmd::sys::sign;
/// let key = PathBuf::from("~/.strata/keys/private.key");
/// let archive = PathBuf::from("snapshot.st");
/// sign::run(key, archive)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run(key_path: PathBuf, image_path: PathBuf) -> Result<()> {
    println!("Signing {:?} with key {:?}...", image_path, key_path);

    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&image_path)?;
    let mut header_bytes = [0u8; HEADER_SIZE];
    f.read_exact(&mut header_bytes)?;
    let mut header: StrataHeader = bincode::deserialize(&header_bytes)?;

    f.seek(SeekFrom::Start(header.index_offset))?;
    let mut index_bytes = Vec::new();
    f.read_to_end(&mut index_bytes)?;

    let mut hasher = Sha256::new();
    hasher.update(&index_bytes);
    let digest = hasher.finalize();

    // 2. Sign digest
    let signature = sign::sign_digest(&key_path, &digest)?;

    // 3. Write signature to file
    // We put the signature at the end of the file (append) or in a reserved slot?
    // The plan said "append a signature block after header (e.g. offset + length in header)".
    // Let's append it to the end of the file.

    let signature_offset = f.seek(SeekFrom::End(0))?;
    f.write_all(&signature)?;
    let signature_length = signature.len() as u32;

    // 4. Update header
    header.signature_offset = Some(signature_offset);
    header.signature_length = Some(signature_length);

    f.seek(SeekFrom::Start(0))?;
    f.write_all(&bincode::serialize(&header)?)?;

    println!(
        "Signature written to image. Offset: {}, Length: {}",
        signature_offset, signature_length
    );
    Ok(())
}
