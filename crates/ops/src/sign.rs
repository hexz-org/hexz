//! Cryptographic signing and verification for archive files.

use hexz_common::Result;
use hexz_common::sign;
use hexz_core::format::header::Header;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Sign a archive's master index with Ed25519.
pub fn sign_archive(image_path: impl AsRef<Path>, key_path: impl AsRef<Path>) -> Result<()> {
    let image_path = image_path.as_ref();
    let key_path = key_path.as_ref();

    let mut f = OpenOptions::new().read(true).write(true).open(image_path)?;
    let mut header = Header::read_from(&mut f)?;

    _ = f.seek(SeekFrom::Start(header.index_offset))?;
    let mut index_bytes = Vec::new();
    _ = f.read_to_end(&mut index_bytes)?;

    let mut hasher = Sha256::new();
    hasher.update(&index_bytes);
    let digest = hasher.finalize();

    let signature = sign::sign_digest(key_path, &digest)?;

    let signature_offset = f.seek(SeekFrom::End(0))?;
    f.write_all(&signature)?;

    header.signature_offset = Some(signature_offset);
    header.signature_length = Some(signature.len() as u32);

    _ = f.seek(SeekFrom::Start(0))?;
    f.write_all(&bincode::serialize(&header)?)?;

    Ok(())
}

/// Verify a archive's Ed25519 signature.
pub fn verify_archive(image_path: impl AsRef<Path>, key_path: impl AsRef<Path>) -> Result<()> {
    let image_path = image_path.as_ref();
    let key_path = key_path.as_ref();

    let mut f = File::open(image_path)?;
    let header = Header::read_from(&mut f)?;

    let (Some(sig_off), Some(sig_len)) = (header.signature_offset, header.signature_length) else {
        return Err(hexz_common::Error::Format(
            "Image is not signed".to_string(),
        ));
    };

    if sig_len != 64 {
        return Err(hexz_common::Error::Format(
            "Invalid signature length".to_string(),
        ));
    }

    let mut signature = [0u8; 64];
    _ = f.seek(SeekFrom::Start(sig_off))?;
    f.read_exact(&mut signature)?;

    let index_len = sig_off - header.index_offset;
    _ = f.seek(SeekFrom::Start(header.index_offset))?;
    let mut index_reader = f.take(index_len);
    let mut index_bytes = Vec::new();
    _ = index_reader.read_to_end(&mut index_bytes)?;

    let mut hasher = Sha256::new();
    hasher.update(&index_bytes);
    let digest = hasher.finalize();

    sign::verify_digest(key_path, &digest, &signature)?;

    Ok(())
}
