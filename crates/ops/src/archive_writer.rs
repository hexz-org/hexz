//! Unified archive write logic.

use hexz_common::Result;
use hexz_common::constants::BLOCK_OFFSET_PARENT;
use hexz_common::crypto::KeyDerivationParams;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;

use crate::write::{create_zero_block, is_zero_chunk, write_block};
use hexz_core::algo::compression::Compressor;
use hexz_core::algo::encryption::Encryptor;
use hexz_core::algo::hashing::ContentHasher;
use hexz_core::algo::hashing::blake3::Blake3Hasher;
use hexz_core::format::{
    header::{CompressionType, FeatureFlags, Header},
    index::{BlockInfo, ENTRIES_PER_PAGE, IndexPage, MasterIndex, PageEntry},
    magic::{FORMAT_VERSION, HEADER_SIZE, MAGIC_BYTES},
};

use crate::parent_index::ParentIndex;
use hexz_core::algo::dedup::hash_table::StandardHashTable;
use hexz_core::api::file::Archive;

pub struct ArchiveWriter {
    out: File,
    current_offset: u64,
    master: MasterIndex,
    global_block_idx: u64,
    dedup_map: StandardHashTable,
    parent_index: Option<ParentIndex>,
    compressor: Box<dyn Compressor>,
    encryptor: Option<Box<dyn Encryptor>>,
    hasher: Blake3Hasher,
    hash_buf: [u8; 32],

    // Reusable buffers
    compress_buf: Vec<u8>,
    encrypt_buf: Vec<u8>,

    // Per-stream page state
    page: IndexPage,
    page_buf: Vec<u8>,
    page_start_block: u64,
    page_start_logical: u64,
    current_logical_pos: u64,
    is_main: bool,
    stream_active: bool,

    // Header metadata
    block_size: u32,
    compression_type: CompressionType,
    variable_blocks: bool,
    encryption_params: Option<KeyDerivationParams>,
    dict_offset: Option<u64>,
    dict_len: Option<u32>,
    cdc_params: Option<(u32, u32, u32)>,
}

pub struct ArchiveWriterBuilder {
    output: std::path::PathBuf,
    compressor: Box<dyn Compressor>,
    compression_type: CompressionType,
    encryptor: Option<Box<dyn Encryptor>>,
    block_size: u32,
    variable_blocks: bool,
    encryption_params: Option<KeyDerivationParams>,
    parent: Option<Arc<Archive>>,
    cdc_params: Option<(u32, u32, u32)>,
}

impl ArchiveWriterBuilder {
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
            parent: None,
            cdc_params: None,
        }
    }

    pub fn cdc_params(mut self, params: Option<(u32, u32, u32)>) -> Self {
        self.cdc_params = params;
        self
    }

    pub fn block_size(mut self, size: u32) -> Self {
        self.block_size = size;
        self
    }

    pub fn variable_blocks(mut self, enabled: bool) -> Self {
        self.variable_blocks = enabled;
        self
    }

    pub fn encryption(
        mut self,
        encryptor: Box<dyn Encryptor>,
        params: KeyDerivationParams,
    ) -> Self {
        self.encryptor = Some(encryptor);
        self.encryption_params = Some(params);
        self
    }

    pub fn compression_type(mut self, compression_type: CompressionType) -> Self {
        self.compression_type = compression_type;
        self
    }

    pub fn parent(mut self, parent: Arc<Archive>) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn build(self) -> Result<ArchiveWriter> {
        let parent_index = if let Some(p) = &self.parent {
            Some(ParentIndex::new(std::slice::from_ref(p))?)
        } else {
            None
        };
        ArchiveWriter::new(
            &self.output,
            self.compressor,
            self.encryptor,
            self.block_size,
            self.compression_type,
            self.variable_blocks,
            self.encryption_params,
            parent_index,
            self.cdc_params,
        )
    }
}

impl ArchiveWriter {
    pub fn builder(
        output: &Path,
        compressor: Box<dyn Compressor>,
        compression_type: CompressionType,
    ) -> ArchiveWriterBuilder {
        ArchiveWriterBuilder::new(output, compressor, compression_type)
    }

    pub fn block_count(&self) -> u64 {
        self.global_block_idx
    }

    pub fn current_logical_pos(&self) -> u64 {
        self.current_logical_pos
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        output: &Path,
        compressor: Box<dyn Compressor>,
        encryptor: Option<Box<dyn Encryptor>>,
        block_size: u32,
        compression_type: CompressionType,
        variable_blocks: bool,
        encryption_params: Option<KeyDerivationParams>,
        parent_index: Option<ParentIndex>,
        cdc_params: Option<(u32, u32, u32)>,
    ) -> Result<Self> {
        let mut out = File::create(output)?;
        out.write_all(&[0u8; HEADER_SIZE])?;

        Ok(Self {
            out,
            current_offset: HEADER_SIZE as u64,
            master: MasterIndex::default(),
            global_block_idx: 0,
            dedup_map: StandardHashTable::with_capacity(4096),
            parent_index,
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
            is_main: true,
            stream_active: false,
            block_size,
            compression_type,
            variable_blocks,
            encryption_params,
            dict_offset: None,
            dict_len: None,
            cdc_params,
        })
    }

    pub fn write_dictionary(&mut self, dict_data: &[u8]) -> Result<()> {
        self.out.write_all(dict_data)?;
        self.dict_offset = Some(self.current_offset);
        self.dict_len = Some(dict_data.len() as u32);
        self.current_offset += dict_data.len() as u64;
        Ok(())
    }

    pub fn begin_stream(&mut self, is_main: bool, total_size: u64) {
        self.is_main = is_main;
        self.stream_active = true;
        self.page = IndexPage::default();
        self.page_start_block = self.global_block_idx;

        if total_size > 0 && self.block_size > 0 {
            let estimated_blocks = (total_size / self.block_size as u64) as usize;
            if estimated_blocks > self.dedup_map.len() {
                self.dedup_map = StandardHashTable::with_capacity(estimated_blocks);
            }
        }

        let stream_start = if is_main {
            self.master.main_size
        } else {
            self.master.auxiliary_size
        };
        self.page_start_logical = stream_start;
        self.current_logical_pos = stream_start;

        if is_main {
            self.master.main_size += total_size;
        } else {
            self.master.auxiliary_size += total_size;
        }
    }

    pub fn write_data_block(&mut self, data: &[u8]) -> Result<()> {
        let chunk_len = data.len() as u32;

        if is_zero_chunk(data) {
            self.page.blocks.push(create_zero_block(chunk_len));
        } else {
            if let Some(p_index) = &self.parent_index {
                let hash = self.hasher.hash_fixed(data);
                if p_index.hashes.contains(&hash) {
                    return self.write_parent_ref(&hash, chunk_len);
                }
            }

            let enc_ref = self.encryptor.as_deref();
            let dedup = if enc_ref.is_some() {
                None
            } else {
                Some(&mut self.dedup_map)
            };
            let info = write_block(
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
            )?;
            self.page.blocks.push(info);
        }

        self.global_block_idx += 1;
        self.current_logical_pos += chunk_len as u64;

        if self.page.blocks.len() >= ENTRIES_PER_PAGE {
            self.flush_page()?;
        }

        Ok(())
    }

    pub fn write_precompressed_block(
        &mut self,
        compressed: &[u8],
        hash: &[u8; 32],
        logical_len: u32,
    ) -> Result<()> {
        if let Some(p_index) = &self.parent_index {
            if p_index.hashes.contains(hash) {
                return self.write_parent_ref(hash, logical_len);
            }
        }

        let checksum = crc32fast::hash(compressed);
        let final_len = compressed.len() as u32;

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
            hash: *hash,
        };

        self.page.blocks.push(info);
        self.global_block_idx += 1;
        self.current_logical_pos += logical_len as u64;

        if self.page.blocks.len() >= ENTRIES_PER_PAGE {
            self.flush_page()?;
        }

        Ok(())
    }

    pub fn write_parent_ref(&mut self, hash: &[u8; 32], logical_len: u32) -> Result<()> {
        self.page.blocks.push(BlockInfo {
            offset: BLOCK_OFFSET_PARENT,
            length: 0,
            logical_len,
            checksum: 0,
            hash: *hash,
        });

        self.global_block_idx += 1;
        self.current_logical_pos += logical_len as u64;

        if self.page.blocks.len() >= ENTRIES_PER_PAGE {
            self.flush_page()?;
        }

        Ok(())
    }

    pub fn end_stream(&mut self) -> Result<()> {
        if !self.page.blocks.is_empty() {
            self.flush_page()?;
        }
        self.stream_active = false;
        Ok(())
    }

    /// Flushes current page to ensure subsequent writes start on a new block.
    /// Used during directory packing to align files to block boundaries.
    pub fn flush_stream(&mut self) -> Result<()> {
        if self.stream_active && !self.page.blocks.is_empty() {
            self.flush_page()?;
        }
        Ok(())
    }

    pub fn finalize(mut self, parent_paths: Vec<String>, metadata: Option<&[u8]>) -> Result<()> {
        if self.stream_active {
            self.end_stream()?;
        }

        let index_offset = self.current_offset;
        let index_bytes = bincode::serialize(&self.master)?;
        self.out.write_all(&index_bytes)?;
        self.current_offset += index_bytes.len() as u64;

        let (meta_offset, meta_len) = if let Some(meta) = metadata {
            let off = self.current_offset;
            self.out.write_all(meta)?;
            self.current_offset += meta.len() as u64;
            (Some(off), Some(meta.len() as u32))
        } else {
            (None, None)
        };

        let header = Header {
            magic: *MAGIC_BYTES,
            version: FORMAT_VERSION,
            block_size: self.block_size,
            index_offset,
            parent_paths,
            dictionary_offset: self.dict_offset,
            dictionary_length: self.dict_len,
            metadata_offset: meta_offset,
            metadata_length: meta_len,
            signature_offset: None,
            signature_length: None,
            encryption: self.encryption_params,
            compression: self.compression_type,
            features: FeatureFlags {
                has_main: !self.master.main_pages.is_empty(),
                has_auxiliary: !self.master.auxiliary_pages.is_empty(),
                variable_blocks: self.variable_blocks,
            },
            cdc_params: self.cdc_params,
        };

        self.out.seek(SeekFrom::Start(0))?;
        self.out.write_all(&bincode::serialize(&header)?)?;
        self.out.flush()?;
        self.out.sync_all()?;

        Ok(())
    }

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

        if self.is_main {
            self.master.main_pages.push(entry);
        } else {
            self.master.auxiliary_pages.push(entry);
        }

        self.page.blocks.clear();
        self.page_start_block = self.global_block_idx;
        self.page_start_logical = self.current_logical_pos;

        Ok(())
    }
}
