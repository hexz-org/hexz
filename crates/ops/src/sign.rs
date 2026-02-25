//! Cryptographic signing and verification for snapshot files.

use hexz_common::Result;
use hexz_common::sign;
use hexz_core::format::header::Header;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Sign a snapshot's master index with Ed25519.
pub fn sign_snapshot(image_path: impl AsRef<Path>, key_path: impl AsRef<Path>) -> Result<()> {
    let image_path = image_path.as_ref();
    let key_path = key_path.as_ref();

    let mut f = OpenOptions::new().read(true).write(true).open(image_path)?;
    let mut header = Header::read_from(&mut f)?;

    f.seek(SeekFrom::Start(header.index_offset))?;
    let mut index_bytes = Vec::new();
    f.read_to_end(&mut index_bytes)?;

    let mut hasher = Sha256::new();
    hasher.update(&index_bytes);
    let digest = hasher.finalize();

    let signature = sign::sign_digest(key_path, &digest)?;

    let signature_offset = f.seek(SeekFrom::End(0))?;
    f.write_all(&signature)?;

    header.signature_offset = Some(signature_offset);
    header.signature_length = Some(signature.len() as u32);

    f.seek(SeekFrom::Start(0))?;
    f.write_all(&bincode::serialize(&header)?)?;

    Ok(())
}

/// Verify a snapshot's Ed25519 signature.
pub fn verify_snapshot(image_path: impl AsRef<Path>, key_path: impl AsRef<Path>) -> Result<()> {
    let image_path = image_path.as_ref();
    let key_path = key_path.as_ref();

    let mut f = File::open(image_path)?;
    let header = Header::read_from(&mut f)?;

    let (sig_off, sig_len) = match (header.signature_offset, header.signature_length) {
        (Some(o), Some(l)) => (o, l),
        _ => {
            return Err(hexz_common::Error::Format(
                "Image is not signed".to_string(),
            ));
        }
    };

    if sig_len != 64 {
        return Err(hexz_common::Error::Format(
            "Invalid signature length".to_string(),
        ));
    }

    let mut signature = [0u8; 64];
    f.seek(SeekFrom::Start(sig_off))?;
    f.read_exact(&mut signature)?;

    let index_len = sig_off - header.index_offset;
    f.seek(SeekFrom::Start(header.index_offset))?;
    let mut index_reader = f.take(index_len);
    let mut index_bytes = Vec::new();
    index_reader.read_to_end(&mut index_bytes)?;

    let mut hasher = Sha256::new();
    hasher.update(&index_bytes);
    let digest = hasher.finalize();

    sign::verify_digest(key_path, &digest, &signature)?;

    Ok(())
}
