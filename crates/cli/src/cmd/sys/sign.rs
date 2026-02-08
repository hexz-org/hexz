//! # Sign Command
//!
//! This module provides the implementation for the `sign` command, which is used to
//! cryptographically sign a Strata image.
//!
//! The signing process involves:
//! 1. Calculating a SHA-256 digest of the Master Index within the image.
//! 2. Signing that digest using a private key provided via a file path.
//! 3. Appending the resulting signature to the end of the image file.
//! 4. Updating the image's header with the signature's offset and length to allow for later verification.

use anyhow::Result;
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use strata_common::sign;
use strata_core::format::header::StrataHeader;
use strata_core::format::magic::HEADER_SIZE;

pub fn run(key_path: PathBuf, image_path: PathBuf) -> Result<()> {
    println!("Signing {:?} with key {:?}...", image_path, key_path);

    // 1. Calculate digest of the *content* (Master Index).
    // The header is mutable (to store the signature), so we sign the immutable index.
    // Ideally we sign (Header - SignatureFields) + Index + Data.
    // For MVP, let's sign the Master Index blob. This ensures the block map is authentic.
    // Data blocks are content-addressed (sort of) or at least checked by index checksums.

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
