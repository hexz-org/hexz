//! Unified snapshot write logic.
//!
//! [`SnapshotWriter`] encapsulates all shared state for writing compressed,
//! deduplicated, and indexed blocks to a Hexz snapshot file. It is the single
//! implementation used by the CLI pack command, the Python builder, and the
//! VM commit command.

use hexz_common::Result;
use hexz_common::constants::BLOCK_OFFSET_PARENT;
use hexz_common::crypto::KeyDerivationParams;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use crate::algo::compression::Compressor;
use crate::algo::encryption::Encryptor;
use crate::format::{
    header::{CompressionType, FeatureFlags, Header},
    index::{BlockInfo, ENTRIES_PER_PAGE, IndexPage, MasterIndex, PageEntry},
    magic::{FORMAT_VERSION, HEADER_SIZE, MAGIC_BYTES},
};
use crate::ops::write::{create_zero_block, is_zero_chunk, write_block};

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
    dedup_map: HashMap<[u8; 32], u64>,
    compressor: Box<dyn Compressor>,
    encryptor: Option<Box<dyn Encryptor>>,

    // Per-stream page state
    page: IndexPage,
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

impl SnapshotWriter {
    /// Creates a new snapshot file and writes the header placeholder.
    pub fn create(
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
            dedup_map: HashMap::new(),
            compressor,
            encryptor,
            page: IndexPage::default(),
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
    /// Must be called before [`write_data_block`] or [`write_parent_ref`].
    /// `total_size` is recorded in the master index.
    pub fn begin_stream(&mut self, is_disk: bool, total_size: u64) {
        self.is_disk = is_disk;
        self.stream_active = true;
        self.page = IndexPage::default();
        self.page_start_block = self.global_block_idx;

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
        let bytes = bincode::serialize(&self.page)?;
        let p_off = self.current_offset;
        self.out.write_all(&bytes)?;
        self.current_offset += bytes.len() as u64;

        let entry = PageEntry {
            offset: p_off,
            length: bytes.len() as u32,
            start_block: self.page_start_block,
            start_logical: self.page_start_logical,
        };

        if self.is_disk {
            self.master.disk_pages.push(entry);
        } else {
            self.master.memory_pages.push(entry);
        }

        self.page = IndexPage::default();
        self.page_start_block = self.global_block_idx;
        self.page_start_logical = self.current_logical_pos;

        Ok(())
    }
}

#[cfg(test)]
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
        let mut w = SnapshotWriter::create(
            &path,
            compressor,
            None,
            4096,
            CompressionType::Lz4,
            false,
            None,
        )
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
        let mut w = SnapshotWriter::create(
            &path,
            compressor,
            None,
            4096,
            CompressionType::Lz4,
            false,
            None,
        )
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
        let mut w = SnapshotWriter::create(
            &path,
            compressor,
            None,
            4096,
            CompressionType::Lz4,
            false,
            None,
        )
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
        let mut w = SnapshotWriter::create(
            &path,
            compressor,
            None,
            4096,
            CompressionType::Lz4,
            false,
            None,
        )
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
