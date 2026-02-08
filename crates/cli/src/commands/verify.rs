use anyhow::Result;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use strata_common::sign;
use strata_core::format::header::StrataHeader;
use strata_core::format::magic::HEADER_SIZE;

pub fn run(key_path: PathBuf, image_path: PathBuf) -> Result<()> {
    println!("Verifying {:?} with key {:?}...", image_path, key_path);

    let mut f = File::open(&image_path)?;
    let mut header_bytes = [0u8; HEADER_SIZE];
    f.read_exact(&mut header_bytes)?;
    let header: StrataHeader = bincode::deserialize(&header_bytes)?;

    let (sig_off, sig_len) = match (header.signature_offset, header.signature_length) {
        (Some(o), Some(l)) => (o, l),
        _ => anyhow::bail!("Image is not signed (missing signature metadata in header)."),
    };

    // Read Signature
    let mut signature = [0u8; 64];
    if sig_len as usize != 64 {
        anyhow::bail!("Invalid signature length: {}", sig_len);
    }
    f.seek(SeekFrom::Start(sig_off))?;
    f.read_exact(&mut signature)?;

    // Read Index (Content)
    // Note: The index ends at sig_off (if we appended it there).
    // Or we read from index_offset to sig_off (if sig is after index).
    // Let's assume index is from `header.index_offset` to `sig_off` (since we appended sig after index).
    let index_len = sig_off - header.index_offset;
    f.seek(SeekFrom::Start(header.index_offset))?;
    let mut index_reader = f.take(index_len);
    let mut index_bytes = Vec::new();
    index_reader.read_to_end(&mut index_bytes)?;

    // Compute Digest
    let mut hasher = Sha256::new();
    hasher.update(&index_bytes);
    let digest = hasher.finalize();

    // Verify
    sign::verify_digest(&key_path, &digest, &signature)?;

    println!("Signature Verified! The image index is authentic.");
    Ok(())
}
