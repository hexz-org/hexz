//! High-level snapshot packing operations.
//!
//! This module contains the core business logic for creating Strata snapshot files
//! from raw disk and memory images. It orchestrates:
//! - Dictionary training (Zstd optimization)
//! - Chunking (fixed-size or content-defined)
//! - Compression and encryption
//! - Deduplication tracking
//! - Hierarchical index building
//!
//! # Architecture
//!
//! The packing process follows this pipeline:
//!
//! ```text
//! ┌─────────────┐
//! │ Input Files │ (disk.raw, mem.raw)
//! └──────┬──────┘
//!        │
//!        ├─ Optional: Dictionary Training (sample blocks → zstd dict)
//!        │
//!        ├─ Chunking (fixed-size OR content-defined via FastCDC)
//!        │
//!        ├─ Compression (LZ4 or Zstd, with optional dictionary)
//!        │
//!        ├─ Optional: Encryption (AES-256-GCM per block)
//!        │
//!        ├─ Deduplication (track hashes, avoid redundant writes)
//!        │
//!        ├─ Write compressed blocks to output file
//!        │
//!        ├─ Build index pages (BlockInfo arrays)
//!        │
//!        ├─ Write index pages to output file
//!        │
//!        └─ Write master index and header
//! ```
//!
//! # Usage
//!
//! This module is designed to be called from:
//! - CLI commands (`strata data pack`)
//! - Python bindings (`strata.pack()`)
//! - Programmatic Rust APIs
//!
//! By keeping it separate from CLI-specific code, we avoid pulling in
//! `clap` and terminal UI dependencies into library contexts.
//!
//! # Examples
//!
//! ```no_run
//! use strata_core::ops::pack::{pack_snapshot, PackConfig};
//! use std::path::PathBuf;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = PackConfig {
//!     disk: Some(PathBuf::from("disk.raw")),
//!     memory: None,
//!     output: PathBuf::from("snapshot.st"),
//!     compression: "lz4".to_string(),
//!     encrypt: false,
//!     password: None,
//!     train_dict: false,
//!     block_size: 65536,
//!     cdc_enabled: false,
//!     ..Default::default()
//! };
//!
//! // Pack with progress reporting
//! pack_snapshot(config, Some(|pos, total| {
//!     println!("Progress: {:.1}%", (pos as f64 / total as f64) * 100.0);
//! }))?;
//! # Ok(())
//! # }
//! ```
//!
//! # Performance
//!
//! - **LZ4**: ~2 GB/s throughput (single-threaded)
//! - **Zstd**: ~500 MB/s (single-threaded, level 3)
//! - **CDC**: ~500 MB/s chunking overhead
//! - **Encryption**: ~1 GB/s (AES-NI acceleration)
//!
//! Packing is mostly CPU-bound (compression). SSD I/O is typically not the bottleneck.

use sha2::Digest;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use strata_common::constants::{DEFAULT_ZSTD_LEVEL, DICT_TRAINING_SIZE, ENTROPY_THRESHOLD};
use strata_common::crypto::KeyDerivationParams;
use strata_common::{Result, StrataError};

use crate::algo::compression::{Compressor, lz4::Lz4Compressor, zstd::ZstdCompressor};
use crate::algo::dedup::cdc::StreamChunker;
use crate::algo::dedup::dcam::DedupeParams;
use crate::algo::encryption::{Encryptor, aes_gcm::AesGcmEncryptor};
use crate::format::{
    header::{CompressionType, FeatureFlags, StrataHeader},
    index::{BlockInfo, ENTRIES_PER_PAGE, IndexPage, MasterIndex, PageEntry},
    magic::{FORMAT_VERSION, HEADER_SIZE, MAGIC_BYTES},
};

/// Configuration parameters for snapshot packing.
///
/// This struct encapsulates all settings for the packing process. It's designed
/// to be easily constructed from CLI arguments or programmatic APIs.
///
/// # Examples
///
/// ```
/// use strata_core::ops::pack::PackConfig;
/// use std::path::PathBuf;
///
/// // Basic configuration with defaults
/// let config = PackConfig {
///     disk: Some(PathBuf::from("disk.img")),
///     output: PathBuf::from("snapshot.st"),
///     ..Default::default()
/// };
///
/// // Advanced configuration with CDC and encryption
/// let advanced = PackConfig {
///     disk: Some(PathBuf::from("disk.img")),
///     output: PathBuf::from("snapshot.st"),
///     compression: "zstd".to_string(),
///     encrypt: true,
///     password: Some("secret".to_string()),
///     cdc_enabled: true,
///     min_chunk: 16384,
///     avg_chunk: 65536,
///     max_chunk: 131072,
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone)]
pub struct PackConfig {
    /// Path to the disk image (optional).
    pub disk: Option<PathBuf>,
    /// Path to the memory image (optional).
    pub memory: Option<PathBuf>,
    /// Output snapshot file path.
    pub output: PathBuf,
    /// Compression algorithm ("lz4" or "zstd").
    pub compression: String,
    /// Enable encryption.
    pub encrypt: bool,
    /// Encryption password (required if encrypt=true).
    pub password: Option<String>,
    /// Train a compression dictionary (zstd only).
    pub train_dict: bool,
    /// Block size in bytes.
    pub block_size: u32,
    /// Enable content-defined chunking (CDC).
    pub cdc_enabled: bool,
    /// Minimum chunk size for CDC.
    pub min_chunk: u32,
    /// Average chunk size for CDC.
    pub avg_chunk: u32,
    /// Maximum chunk size for CDC.
    pub max_chunk: u32,
}

impl Default for PackConfig {
    fn default() -> Self {
        Self {
            disk: None,
            memory: None,
            output: PathBuf::from("output.st"),
            compression: "lz4".to_string(),
            encrypt: false,
            password: None,
            train_dict: false,
            block_size: 65536,
            cdc_enabled: false,
            min_chunk: 16384,
            avg_chunk: 65536,
            max_chunk: 131072,
        }
    }
}

/// Calculates Shannon entropy of a byte slice.
///
/// Shannon entropy measures the "randomness" or information content of data:
/// - **0.0**: All bytes are identical (highly compressible)
/// - **8.0**: Maximum entropy, random data (incompressible)
///
/// # Formula
///
/// ```text
/// H(X) = -Σ p(x) * log2(p(x))
/// ```
///
/// Where `p(x)` is the frequency of each byte value.
///
/// # Usage
///
/// Used during dictionary training to filter out high-entropy (random) blocks
/// that wouldn't benefit from compression. Only blocks with entropy below
/// `ENTROPY_THRESHOLD` are included in the training set.
///
/// # Parameters
///
/// - `data`: Byte slice to analyze
///
/// # Returns
///
/// Entropy value from 0.0 (homogeneous) to 8.0 (random).
///
/// # Examples
///
/// ```
/// # use strata_core::ops::pack::calculate_entropy;
/// // Homogeneous data (low entropy)
/// let zeros = vec![0u8; 1024];
/// let entropy = calculate_entropy(&zeros);
/// assert_eq!(entropy, 0.0);
///
/// // Random data (high entropy)
/// let random: Vec<u8> = (0..=255).cycle().take(1024).collect();
/// let entropy = calculate_entropy(&random);
/// assert!(entropy > 7.0);
/// ```
pub fn calculate_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    let mut frequencies = [0u32; 256];
    for &byte in data {
        frequencies[byte as usize] += 1;
    }

    let len = data.len() as f64;
    let mut entropy = 0.0;

    for &count in frequencies.iter() {
        if count > 0 {
            let p = count as f64 / len;
            entropy -= p * p.log2();
        }
    }

    entropy
}

/// Trait for chunk iterators (fixed-size or content-defined).
///
/// This trait provides a unified interface for both fixed-size and CDC chunkers,
/// allowing the packing logic to be agnostic to the chunking strategy.
trait Chunker: Iterator<Item = std::io::Result<Vec<u8>>> {}
impl<T: Iterator<Item = std::io::Result<Vec<u8>>>> Chunker for T {}

/// Fixed-size block chunker.
///
/// Splits input into equal-sized blocks (except possibly the last one).
/// Simpler and faster than CDC, but less effective for deduplication.
///
/// # Performance
///
/// - **Throughput**: ~3 GB/s (limited by memory copy)
/// - **Chunk variance**: None (all chunks are `block_size`, except last)
struct FixedChunker<R> {
    /// Input data source.
    reader: R,
    /// Fixed block size in bytes.
    block_size: usize,
}

impl<R: Read> FixedChunker<R> {
    /// Creates a new fixed-size chunker.
    ///
    /// # Parameters
    ///
    /// - `reader`: Input data source
    /// - `block_size`: Size of each chunk in bytes
    fn new(reader: R, block_size: usize) -> Self {
        Self { reader, block_size }
    }
}

impl<R: Read> Iterator for FixedChunker<R> {
    type Item = std::io::Result<Vec<u8>>;

    /// Yields the next fixed-size chunk.
    ///
    /// # Returns
    ///
    /// - `Some(Ok(chunk))`: Next chunk (may be shorter than `block_size` for last chunk)
    /// - `Some(Err(e))`: I/O error reading from source
    /// - `None`: End of input reached
    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = vec![0u8; self.block_size];
        match self.reader.read(&mut buf) {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                Some(Ok(buf))
            }
            Err(e) => Some(Err(e)),
        }
    }
}

/// Packs a snapshot file from disk and/or memory images.
///
/// This is the main entry point for creating Strata snapshot files. It handles:
/// - Dictionary training (for zstd compression)
/// - Chunking (fixed-size or content-defined)
/// - Compression and encryption
/// - Deduplication
/// - Index building
///
/// # Arguments
///
/// * `config` - Packing configuration parameters
/// * `progress_callback` - Optional callback for progress reporting (logical_pos, total_size)
///
/// # Returns
///
/// Returns `Ok(())` on success, or an error if packing fails.
pub fn pack_snapshot<F>(config: PackConfig, progress_callback: Option<F>) -> Result<()>
where
    F: Fn(u64, u64) + Send + Sync,
{
    // Validate inputs
    if config.disk.is_none() && config.memory.is_none() {
        return Err(StrataError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "At least one input (disk or memory) must be provided",
        )));
    }

    let mut out = File::create(&config.output)?;
    out.write_all(&[0u8; HEADER_SIZE])?;

    // Train compression dictionary if requested
    let dictionary = if config.compression == "zstd" && config.train_dict {
        Some(train_dictionary(
            config.disk.as_ref().or(config.memory.as_ref()).unwrap(),
            config.block_size,
        )?)
    } else {
        None
    };

    let mut current_offset = HEADER_SIZE as u64;

    // Write dictionary to file
    let (dict_offset, dict_len) = if let Some(d) = &dictionary {
        out.write_all(d)?;
        let start = current_offset;
        let len = d.len() as u32;
        current_offset += len as u64;
        (Some(start), Some(len))
    } else {
        (None, None)
    };

    // Initialize compressor
    let compressor: Box<dyn Compressor> = match config.compression.as_str() {
        "zstd" => Box::new(ZstdCompressor::new(DEFAULT_ZSTD_LEVEL, dictionary)),
        _ => Box::new(Lz4Compressor::new()),
    };

    // Initialize encryptor if requested
    let (encryptor, enc_header) = if config.encrypt {
        let password = config.password.clone().ok_or_else(|| {
            StrataError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Password required for encryption",
            ))
        })?;
        let params = KeyDerivationParams::default();
        let enc = AesGcmEncryptor::new(password.as_bytes(), &params.salt, params.iterations);
        (Some(enc), Some(params))
    } else {
        (None, None)
    };

    let mut master = MasterIndex::default();
    let mut dedup_map: HashMap<[u8; 32], u64> = HashMap::new();
    let mut global_block_idx = 0;

    // Process disk stream
    if let Some(ref path) = config.disk {
        process_stream(
            path.clone(),
            true,
            &mut out,
            &mut current_offset,
            &mut master,
            &mut global_block_idx,
            &mut dedup_map,
            compressor.as_ref(),
            &encryptor,
            &config,
            progress_callback.as_ref(),
        )?;
    }

    // Process memory stream
    if let Some(ref path) = config.memory {
        process_stream(
            path.clone(),
            false,
            &mut out,
            &mut current_offset,
            &mut master,
            &mut global_block_idx,
            &mut dedup_map,
            compressor.as_ref(),
            &encryptor,
            &config,
            progress_callback.as_ref(),
        )?;
    }

    // Write master index
    let index_offset = current_offset;
    let index_bytes = bincode::serialize(&master)?;
    out.write_all(&index_bytes)?;

    // Write header
    let header = StrataHeader {
        magic: *MAGIC_BYTES,
        version: FORMAT_VERSION,
        block_size: config.block_size,
        index_offset,
        parent_path: None,
        dictionary_offset: dict_offset,
        dictionary_length: dict_len,
        metadata_offset: None,
        metadata_length: None,
        signature_offset: None,
        signature_length: None,
        encryption: enc_header,
        compression: if config.compression == "zstd" {
            CompressionType::Zstd
        } else {
            CompressionType::Lz4
        },
        features: FeatureFlags {
            has_disk: !master.disk_pages.is_empty(),
            has_memory: !master.memory_pages.is_empty(),
            variable_blocks: config.cdc_enabled,
        },
    };

    out.seek(SeekFrom::Start(0))?;
    out.write_all(&bincode::serialize(&header)?)?;
    out.flush()?;

    Ok(())
}

/// Trains a compression dictionary from the input file.
fn train_dictionary(input_path: &Path, block_size: u32) -> Result<Vec<u8>> {
    let mut f = File::open(input_path)?;
    let file_len = f.metadata()?.len();

    let mut samples = Vec::new();
    let mut buffer = vec![0u8; block_size as usize];
    let target_samples = DICT_TRAINING_SIZE;

    let step = if file_len > 0 {
        (file_len / target_samples as u64).max(block_size as u64)
    } else {
        0
    };

    let mut attempts = 0;
    while samples.len() < target_samples && attempts < target_samples * 2 {
        let offset = attempts as u64 * step;
        if offset >= file_len {
            break;
        }

        f.seek(SeekFrom::Start(offset))?;
        let n = f.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        let chunk = &buffer[..n];
        let is_zeros = chunk.iter().all(|&b| b == 0);

        if !is_zeros {
            let entropy = calculate_entropy(chunk);
            if entropy < ENTROPY_THRESHOLD {
                samples.push(chunk.to_vec());
            }
        }
        attempts += 1;
    }

    if samples.is_empty() {
        tracing::warn!("Input seems to be empty or high entropy. Dictionary will be empty.");
        Ok(Vec::new())
    } else {
        let dict_bytes = ZstdCompressor::train(&samples, DICT_TRAINING_SIZE)?;
        tracing::info!("Dictionary trained: {} bytes", dict_bytes.len());
        Ok(dict_bytes)
    }
}

/// Processes a single stream (disk or memory).
#[allow(clippy::too_many_arguments)]
fn process_stream<F>(
    path: PathBuf,
    is_disk: bool,
    out: &mut File,
    current_offset: &mut u64,
    master: &mut MasterIndex,
    global_block_idx: &mut u64,
    dedup_map: &mut HashMap<[u8; 32], u64>,
    compressor: &dyn Compressor,
    encryptor: &Option<AesGcmEncryptor>,
    config: &PackConfig,
    progress_callback: Option<&F>,
) -> Result<()>
where
    F: Fn(u64, u64),
{
    let f = File::open(&path)?;
    let len = f.metadata()?.len();

    if is_disk {
        master.disk_size = len;
    } else {
        master.memory_size = len;
    }

    let mut page = IndexPage::default();
    let mut page_start_block = *global_block_idx;
    let mut page_start_logical = 0u64;
    let mut current_logical_pos = 0u64;

    // Choose chunker based on configuration
    let chunker: Box<dyn Chunker> = if config.cdc_enabled {
        let params = DedupeParams {
            f: (config.avg_chunk as f64).log2() as u32,
            m: config.min_chunk,
            z: config.max_chunk,
            w: 48,
            v: 8,
        };
        Box::new(StreamChunker::new(f, params))
    } else {
        Box::new(FixedChunker::new(f, config.block_size as usize))
    };

    for chunk_res in chunker {
        let chunk = chunk_res?;
        let chunk_len = chunk.len() as u32;

        // Handle zero blocks efficiently
        if chunk.iter().all(|&b| b == 0) {
            page.blocks.push(BlockInfo {
                offset: 0,
                length: 0,
                logical_len: chunk_len,
                checksum: 0,
            });
        } else {
            // Compress the chunk
            let compressed = compressor.compress(&chunk)?;

            // Encrypt if requested
            let final_data = if let Some(enc) = encryptor {
                enc.encrypt(&compressed, *global_block_idx)?
            } else {
                compressed
            };

            let checksum = crc32fast::hash(&final_data);
            let offset;

            // Handle deduplication (disabled for encrypted data)
            if config.encrypt {
                offset = *current_offset;
                out.write_all(&final_data)?;
                *current_offset += final_data.len() as u64;
            } else {
                let hash = sha2::Sha256::digest(&final_data);
                let hash_key: [u8; 32] = hash.into();

                if let Some(&off) = dedup_map.get(&hash_key) {
                    offset = off;
                } else {
                    offset = *current_offset;
                    dedup_map.insert(hash_key, offset);
                    out.write_all(&final_data)?;
                    *current_offset += final_data.len() as u64;
                }
            }

            page.blocks.push(BlockInfo {
                offset,
                length: final_data.len() as u32,
                logical_len: chunk_len,
                checksum,
            });
        }

        *global_block_idx += 1;
        current_logical_pos += chunk_len as u64;

        // Report progress
        if let Some(callback) = progress_callback {
            callback(current_logical_pos, len);
        }

        // Flush page if full
        if page.blocks.len() >= ENTRIES_PER_PAGE {
            let bytes = bincode::serialize(&page)?;
            let p_off = *current_offset;
            out.write_all(&bytes)?;
            *current_offset += bytes.len() as u64;

            let entry = PageEntry {
                offset: p_off,
                length: bytes.len() as u32,
                start_block: page_start_block,
                start_logical: page_start_logical,
            };

            if is_disk {
                master.disk_pages.push(entry);
            } else {
                master.memory_pages.push(entry);
            }

            page = IndexPage::default();
            page_start_block = *global_block_idx;
            page_start_logical = current_logical_pos;
        }
    }

    // Flush remaining page
    if !page.blocks.is_empty() {
        let bytes = bincode::serialize(&page)?;
        let p_off = *current_offset;
        out.write_all(&bytes)?;
        *current_offset += bytes.len() as u64;

        let entry = PageEntry {
            offset: p_off,
            length: bytes.len() as u32,
            start_block: page_start_block,
            start_logical: page_start_logical,
        };

        if is_disk {
            master.disk_pages.push(entry);
        } else {
            master.memory_pages.push(entry);
        }
    }

    Ok(())
}
