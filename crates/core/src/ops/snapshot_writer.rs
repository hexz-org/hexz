//! Unified snapshot write logic.
//!
//! [`SnapshotWriter`] encapsulates all shared state for writing compressed,
//! deduplicated, and indexed blocks to a Hexz snapshot file. It is the single
//! implementation used by the CLI pack command, the Python builder, and the
//! VM commit command.

use hexz_common::Result;
use hexz_common::constants::BLOCK_OFFSET_PARENT;
use hexz_common::crypto::KeyDerivationParams;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use crate::algo::compression::Compressor;
use crate::algo::encryption::Encryptor;
use crate::algo::hashing::blake3::Blake3Hasher;
use crate::format::{
    header::{CompressionType, FeatureFlags, Header},
    index::{BlockInfo, ENTRIES_PER_PAGE, IndexPage, MasterIndex, PageEntry},
    magic::{FORMAT_VERSION, HEADER_SIZE, MAGIC_BYTES},
};
use crate::ops::write::{create_zero_block, is_zero_chunk, write_block};

use crate::algo::dedup::hash_table::StandardHashTable;

/// Unified writer for Hexz snapshot files.
///
/// Owns the output file, compressor, optional encryptor, dedup map, and all
/// index-page bookkeeping. Callers feed it data blocks; it handles zero
/// detection, compression, encryption, deduplication, and index management.
pub struct SnapshotWriter {
    out: File,
    current_offset: u64,
    master: MasterIndex,
    global_block_idx: u64,
    dedup_map: StandardHashTable,
    compressor: Box<dyn Compressor>,
    encryptor: Option<Box<dyn Encryptor>>,
    hasher: Blake3Hasher,
    hash_buf: [u8; 32],

    // Reusable buffers for compression and encryption
    compress_buf: Vec<u8>,
    encrypt_buf: Vec<u8>,

    // Per-stream page state
    page: IndexPage,
    page_buf: Vec<u8>,
    page_start_block: u64,
    page_start_logical: u64,
    current_logical_pos: u64,
    is_disk: bool,
    stream_active: bool,

    // Header metadata
    block_size: u32,
    compression_type: CompressionType,
    variable_blocks: bool,
    encryption_params: Option<KeyDerivationParams>,
    dict_offset: Option<u64>,
    dict_len: Option<u32>,
}

/// Builder for configuring and creating a `SnapshotWriter`.
///
/// Provides a fluent API for constructing snapshots with optional features
/// like encryption, variable-sized blocks, and custom block sizes.
///
/// # Examples
///
/// ```no_run
/// use hexz_core::ops::snapshot_writer::SnapshotWriterBuilder;
/// use hexz_core::algo::compression::{Compressor, lz4::Lz4Compressor};
/// use hexz_core::format::header::CompressionType;
/// use std::path::Path;
///
/// let compressor: Box<dyn Compressor> = Box::new(Lz4Compressor::new());
/// let writer = SnapshotWriterBuilder::new(
///     Path::new("output.hxz"),
///     compressor,
///     CompressionType::Lz4,
/// )
///     .block_size(65536)
///     .variable_blocks(true)
///     .build()?;
/// # Ok::<(), hexz_common::Error>(())
/// ```
pub struct SnapshotWriterBuilder {
    output: std::path::PathBuf,
    compressor: Box<dyn Compressor>,
    compression_type: CompressionType,
    encryptor: Option<Box<dyn Encryptor>>,
    block_size: u32,
    variable_blocks: bool,
    encryption_params: Option<KeyDerivationParams>,
}

impl SnapshotWriterBuilder {
    /// Creates a new builder for a snapshot file.
    ///
    /// **Required parameters:**
    /// - `output`: Path where the snapshot will be written
    /// - `compressor`: Compression algorithm (LZ4 or Zstd)
    /// - `compression_type`: The compression type (must match the compressor)
    ///
    /// **Default values:**
    /// - Block size: 65536 (64 KiB)
    /// - Variable blocks: false
    /// - Encryption: disabled
    pub fn new(
        output: &Path,
        compressor: Box<dyn Compressor>,
        compression_type: CompressionType,
    ) -> Self {
        Self {
            output: output.to_path_buf(),
            compressor,
            compression_type,
            encryptor: None,
            block_size: 65536,
            variable_blocks: false,
            encryption_params: None,
        }
    }

    /// Sets the block size in bytes (default: 65536).
    pub fn block_size(mut self, size: u32) -> Self {
        self.block_size = size;
        self
    }

    /// Enables variable-sized blocks for CDC-based deduplication (default: false).
    pub fn variable_blocks(mut self, enabled: bool) -> Self {
        self.variable_blocks = enabled;
        self
    }

    /// Sets the encryption parameters and encryptor.
    pub fn encryption(
        mut self,
        encryptor: Box<dyn Encryptor>,
        params: KeyDerivationParams,
    ) -> Self {
        self.encryptor = Some(encryptor);
        self.encryption_params = Some(params);
        self
    }

    /// Explicitly sets the compression type (usually auto-detected).
    pub fn compression_type(mut self, compression_type: CompressionType) -> Self {
        self.compression_type = compression_type;
        self
    }

    /// Builds the `SnapshotWriter` and creates the output file.
    pub fn build(self) -> Result<SnapshotWriter> {
        SnapshotWriter::new(
            &self.output,
            self.compressor,
            self.encryptor,
            self.block_size,
            self.compression_type,
            self.variable_blocks,
            self.encryption_params,
        )
    }
}

impl SnapshotWriter {
    /// Creates a builder for constructing a snapshot writer.
    pub fn builder(
        output: &Path,
        compressor: Box<dyn Compressor>,
        compression_type: CompressionType,
    ) -> SnapshotWriterBuilder {
        SnapshotWriterBuilder::new(output, compressor, compression_type)
    }

    /// Internal constructor called by the builder. Do not use directly.
    fn new(
        output: &Path,
        compressor: Box<dyn Compressor>,
        encryptor: Option<Box<dyn Encryptor>>,
        block_size: u32,
        compression_type: CompressionType,
        variable_blocks: bool,
        encryption_params: Option<KeyDerivationParams>,
    ) -> Result<Self> {
        let mut out = File::create(output)?;
        out.write_all(&[0u8; HEADER_SIZE])?;

        Ok(Self {
            out,
            current_offset: HEADER_SIZE as u64,
            master: MasterIndex::default(),
            global_block_idx: 0,
            dedup_map: StandardHashTable::with_capacity(4096),
            compressor,
            encryptor,
            hasher: Blake3Hasher,
            hash_buf: [0u8; 32],
            compress_buf: Vec::new(),
            encrypt_buf: Vec::new(),
            page: IndexPage::default(),
            page_buf: Vec::new(),
            page_start_block: 0,
            page_start_logical: 0,
            current_logical_pos: 0,
            is_disk: true,
            stream_active: false,
            block_size,
            compression_type,
            variable_blocks,
            encryption_params,
            dict_offset: None,
            dict_len: None,
        })
    }

    /// Writes a trained dictionary immediately after the header.
    pub fn write_dictionary(&mut self, dict_data: &[u8]) -> Result<()> {
        self.out.write_all(dict_data)?;
        self.dict_offset = Some(self.current_offset);
        self.dict_len = Some(dict_data.len() as u32);
        self.current_offset += dict_data.len() as u64;
        Ok(())
    }

    /// Begins a new stream (disk or memory).
    ///
    /// Must be called before writing blocks or parent references.
    /// `total_size` is recorded in the master index and used to pre-size the
    /// dedup map for reduced rehashing.
    pub fn begin_stream(&mut self, is_disk: bool, total_size: u64) {
        self.is_disk = is_disk;
        self.stream_active = true;
        self.page = IndexPage::default();
        self.page_start_block = self.global_block_idx;

        // Pre-size dedup map based on estimated block count
        if total_size > 0 && self.block_size > 0 {
            let estimated_blocks = (total_size / self.block_size as u64) as usize;
            if estimated_blocks > self.dedup_map.len() {
                self.dedup_map = StandardHashTable::with_capacity(estimated_blocks);
            }
        }

        // Continue logical positions from the end of previous streams of the same type.
        let stream_start = if is_disk {
            self.master.disk_size
        } else {
            self.master.memory_size
        };
        self.page_start_logical = stream_start;
        self.current_logical_pos = stream_start;

        if is_disk {
            self.master.disk_size += total_size;
        } else {
            self.master.memory_size += total_size;
        }
    }

    /// Writes a data block: zero-detect → compress → encrypt → dedup → index.
    pub fn write_data_block(&mut self, data: &[u8]) -> Result<()> {
        let chunk_len = data.len() as u32;

        let info = if is_zero_chunk(data) {
            create_zero_block(chunk_len)
        } else {
            let enc_ref = self.encryptor.as_deref();
            let dedup = if enc_ref.is_some() {
                None
            } else {
                Some(&mut self.dedup_map)
            };
            write_block(
                &mut self.out,
                data,
                self.global_block_idx,
                &mut self.current_offset,
                dedup,
                self.compressor.as_ref(),
                enc_ref,
                &self.hasher,
                &mut self.hash_buf,
                &mut self.compress_buf,
                &mut self.encrypt_buf,
            )?
        };

        self.page.blocks.push(info);
        self.global_block_idx += 1;
        self.current_logical_pos += chunk_len as u64;

        if self.page.blocks.len() >= ENTRIES_PER_PAGE {
            self.flush_page()?;
        }

        Ok(())
    }

    /// Writes a pre-compressed block: dedup → write → index.
    ///
    /// Used by the parallel packing pipeline where compression happens
    /// on worker threads. The caller provides already-compressed data
    /// and its content hash.
    pub fn write_precompressed_block(
        &mut self,
        compressed: &[u8],
        hash: &[u8; 32],
        logical_len: u32,
    ) -> Result<()> {
        let checksum = crc32fast::hash(compressed);
        let final_len = compressed.len() as u32;

        // Dedup check using the provided hash
        let offset = if let Some(existing_offset) = self.dedup_map.get(hash) {
            existing_offset
        } else {
            let off = self.current_offset;
            self.dedup_map.insert(*hash, off);
            self.out.write_all(compressed)?;
            self.current_offset += final_len as u64;
            off
        };

        let info = BlockInfo {
            offset,
            length: final_len,
            logical_len,
            checksum,
        };

        self.page.blocks.push(info);
        self.global_block_idx += 1;
        self.current_logical_pos += logical_len as u64;

        if self.page.blocks.len() >= ENTRIES_PER_PAGE {
            self.flush_page()?;
        }

        Ok(())
    }

    /// Writes a parent-reference marker for thin snapshots.
    pub fn write_parent_ref(&mut self, logical_len: u32) -> Result<()> {
        self.page.blocks.push(BlockInfo {
            offset: BLOCK_OFFSET_PARENT,
            length: 0,
            logical_len,
            checksum: 0,
        });

        self.global_block_idx += 1;
        self.current_logical_pos += logical_len as u64;

        if self.page.blocks.len() >= ENTRIES_PER_PAGE {
            self.flush_page()?;
        }

        Ok(())
    }

    /// Ends the current stream, flushing any remaining index page.
    pub fn end_stream(&mut self) -> Result<()> {
        if !self.page.blocks.is_empty() {
            self.flush_page()?;
        }
        self.stream_active = false;
        Ok(())
    }

    /// Writes master index + header, consuming the writer.
    pub fn finalize(mut self, parent_path: Option<String>, metadata: Option<&[u8]>) -> Result<()> {
        // If a stream is still active, end it
        if self.stream_active {
            self.end_stream()?;
        }

        // Write master index
        let index_offset = self.current_offset;
        let index_bytes = bincode::serialize(&self.master)?;
        self.out.write_all(&index_bytes)?;
        self.current_offset += index_bytes.len() as u64;

        // Write metadata if present
        let (meta_offset, meta_len) = if let Some(meta) = metadata {
            let off = self.current_offset;
            self.out.write_all(meta)?;
            self.current_offset += meta.len() as u64;
            (Some(off), Some(meta.len() as u32))
        } else {
            (None, None)
        };

        // Build and write header
        let header = Header {
            magic: *MAGIC_BYTES,
            version: FORMAT_VERSION,
            block_size: self.block_size,
            index_offset,
            parent_path,
            dictionary_offset: self.dict_offset,
            dictionary_length: self.dict_len,
            metadata_offset: meta_offset,
            metadata_length: meta_len,
            signature_offset: None,
            signature_length: None,
            encryption: self.encryption_params,
            compression: self.compression_type,
            features: FeatureFlags {
                has_disk: !self.master.disk_pages.is_empty(),
                has_memory: !self.master.memory_pages.is_empty(),
                variable_blocks: self.variable_blocks,
            },
        };

        self.out.seek(SeekFrom::Start(0))?;
        self.out.write_all(&bincode::serialize(&header)?)?;
        self.out.flush()?;
        self.out.sync_all()?;

        Ok(())
    }

    /// Returns the number of blocks written so far (across all streams).
    pub fn block_count(&self) -> u64 {
        self.global_block_idx
    }

    /// Returns the current physical file offset.
    pub fn current_offset(&self) -> u64 {
        self.current_offset
    }

    // -- private helpers --

    fn flush_page(&mut self) -> Result<()> {
        self.page_buf.clear();
        bincode::serialize_into(&mut self.page_buf, &self.page)?;
        let p_off = self.current_offset;
        self.out.write_all(&self.page_buf)?;
        self.current_offset += self.page_buf.len() as u64;

        let entry = PageEntry {
            offset: p_off,
            length: self.page_buf.len() as u32,
            start_block: self.page_start_block,
            start_logical: self.page_start_logical,
        };

        if self.is_disk {
            self.master.disk_pages.push(entry);
        } else {
            self.master.memory_pages.push(entry);
        }

        // Clear blocks but preserve Vec capacity for next page
        self.page.blocks.clear();
        self.page_start_block = self.global_block_idx;
        self.page_start_logical = self.current_logical_pos;

        Ok(())
    }
}

#[cfg(test)]
#[allow(deprecated)] // Tests use deprecated create() method for brevity
mod tests {
    use super::*;
    use crate::algo::compression::lz4::Lz4Compressor;
    use crate::format::header::Header;
    use std::io::Read;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_path() -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!("hexz_sw_{}_{}.hxz", std::process::id(), id));
        p
    }

    #[test]
    fn test_round_trip_simple() {
        let path = temp_path();
        let compressor: Box<dyn Compressor> = Box::new(Lz4Compressor::new());
        let mut w = SnapshotWriter::builder(&path, compressor, CompressionType::Lz4)
            .block_size(4096)
            .build()
            .unwrap();

        w.begin_stream(true, 8192);
        w.write_data_block(&vec![0xAA; 4096]).unwrap();
        w.write_data_block(&vec![0u8; 4096]).unwrap(); // zero block
        w.end_stream().unwrap();
        w.finalize(None, None).unwrap();

        // Verify file is readable
        let mut f = File::open(&path).unwrap();
        let mut header_buf = vec![0u8; HEADER_SIZE];
        f.read_exact(&mut header_buf).unwrap();
        let header: Header = bincode::deserialize(&header_buf).unwrap();
        assert_eq!(&header.magic, MAGIC_BYTES);
        assert_eq!(header.block_size, 4096);
        assert!(!header.features.has_memory);
        assert!(header.features.has_disk);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_parent_ref() {
        let path = temp_path();
        let compressor: Box<dyn Compressor> = Box::new(Lz4Compressor::new());
        let mut w = SnapshotWriter::builder(&path, compressor, CompressionType::Lz4)
            .block_size(4096)
            .build()
            .unwrap();

        w.begin_stream(true, 4096);
        w.write_parent_ref(4096).unwrap();
        w.end_stream().unwrap();
        w.finalize(Some("/parent.hxz".to_string()), None).unwrap();

        let mut f = File::open(&path).unwrap();
        let mut header_buf = vec![0u8; HEADER_SIZE];
        f.read_exact(&mut header_buf).unwrap();
        let header: Header = bincode::deserialize(&header_buf).unwrap();
        assert_eq!(header.parent_path.as_deref(), Some("/parent.hxz"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_dedup() {
        let path = temp_path();
        let compressor: Box<dyn Compressor> = Box::new(Lz4Compressor::new());
        let mut w = SnapshotWriter::builder(&path, compressor, CompressionType::Lz4)
            .block_size(4096)
            .build()
            .unwrap();

        w.begin_stream(true, 12288);
        let chunk = vec![0xBB; 4096];
        w.write_data_block(&chunk).unwrap();
        let offset_after_first = w.current_offset();
        w.write_data_block(&chunk).unwrap(); // duplicate
        let offset_after_second = w.current_offset();
        assert_eq!(
            offset_after_first, offset_after_second,
            "dedup should skip write"
        );
        w.write_data_block(&vec![0xCC; 4096]).unwrap(); // unique
        w.end_stream().unwrap();
        w.finalize(None, None).unwrap();

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_metadata() {
        let path = temp_path();
        let compressor: Box<dyn Compressor> = Box::new(Lz4Compressor::new());
        let mut w = SnapshotWriter::builder(&path, compressor, CompressionType::Lz4)
            .block_size(4096)
            .build()
            .unwrap();

        w.begin_stream(true, 4096);
        w.write_data_block(&vec![1u8; 4096]).unwrap();
        w.end_stream().unwrap();

        let meta = b"test metadata";
        w.finalize(None, Some(meta)).unwrap();

        let mut f = File::open(&path).unwrap();
        let mut header_buf = vec![0u8; HEADER_SIZE];
        f.read_exact(&mut header_buf).unwrap();
        let header: Header = bincode::deserialize(&header_buf).unwrap();
        assert!(header.metadata_offset.is_some());
        assert_eq!(header.metadata_length, Some(meta.len() as u32));

        let _ = std::fs::remove_file(&path);
    }
}
