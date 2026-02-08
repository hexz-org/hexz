//! Low-level write operations for Strata snapshots.
//!
//! This module provides building blocks for writing compressed, encrypted,
//! and deduplicated blocks to snapshot files. These functions are used by
//! the higher-level pack operations.

use std::collections::HashMap;
use std::io::Write;
use strata_common::Result;

use crate::algo::compression::Compressor;
use crate::algo::encryption::Encryptor;
use crate::format::index::BlockInfo;

/// Writes a compressed and optionally encrypted block to the output stream.
///
/// # Arguments
///
/// * `out` - The output writer
/// * `chunk` - The uncompressed chunk data
/// * `block_idx` - The global block index (used for encryption nonce)
/// * `current_offset` - The current file offset (will be updated)
/// * `dedup_map` - Optional deduplication map (disabled if encrypting)
/// * `compressor` - The compression algorithm to use
/// * `encryptor` - Optional encryptor
///
/// # Returns
///
/// Returns a `BlockInfo` describing the written block.
pub fn write_block<W: Write>(
    out: &mut W,
    chunk: &[u8],
    block_idx: u64,
    current_offset: &mut u64,
    dedup_map: Option<&mut HashMap<[u8; 32], u64>>,
    compressor: &dyn Compressor,
    encryptor: Option<&dyn Encryptor>,
) -> Result<BlockInfo> {
    use sha2::Digest;

    // Compress the chunk
    let compressed = compressor.compress(chunk)?;

    // Encrypt if requested
    let final_data = if let Some(enc) = encryptor {
        enc.encrypt(&compressed, block_idx)?
    } else {
        compressed
    };

    let checksum = crc32fast::hash(&final_data);
    let chunk_len = chunk.len() as u32;

    // Handle deduplication (only if not encrypting)
    let offset = if encryptor.is_some() {
        // No dedup for encrypted data
        let off = *current_offset;
        out.write_all(&final_data)?;
        *current_offset += final_data.len() as u64;
        off
    } else if let Some(map) = dedup_map {
        // Try to deduplicate
        let hash = sha2::Sha256::digest(&final_data);
        let hash_key: [u8; 32] = hash.into();

        if let Some(&existing_offset) = map.get(&hash_key) {
            // Block already exists, reuse it
            existing_offset
        } else {
            // New block, write it and record in map
            let off = *current_offset;
            map.insert(hash_key, off);
            out.write_all(&final_data)?;
            *current_offset += final_data.len() as u64;
            off
        }
    } else {
        // No dedup, just write
        let off = *current_offset;
        out.write_all(&final_data)?;
        *current_offset += final_data.len() as u64;
        off
    };

    Ok(BlockInfo {
        offset,
        length: final_data.len() as u32,
        logical_len: chunk_len,
        checksum,
    })
}

/// Creates a zero-block descriptor without writing data.
///
/// Zero blocks are represented as having offset=0 and length=0,
/// which signals to the reader to return zeros without storage.
pub fn create_zero_block(logical_len: u32) -> BlockInfo {
    BlockInfo {
        offset: 0,
        length: 0,
        logical_len,
        checksum: 0,
    }
}

/// Checks if a chunk consists entirely of zero bytes.
pub fn is_zero_chunk(chunk: &[u8]) -> bool {
    chunk.iter().all(|&b| b == 0)
}
