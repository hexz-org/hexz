//! Python class for building Strata snapshots.
//!
//! Provides the low-level `StrataBuilder` that can create archives from
//! disk images, memory dumps, and overlay merges.
//!
//! # Python Usage Examples
//!
//! ## Basic Snapshot Creation
//!
//! ```python
//! from strata import StrataBuilder
//!
//! # Create a new snapshot with LZ4 compression
//! builder = StrataBuilder("output.st", compression="lz4")
//! builder.add_disk_file("disk.img")
//! builder.finalize()
//! ```
//!
//! ## Full VM Snapshot with Memory
//!
//! ```python
//! from strata import StrataBuilder
//!
//! # Create snapshot with both disk and memory
//! builder = StrataBuilder("vm-snapshot.st", compression="zstd", compression_level=5)
//! builder.add_disk_file("disk.img")
//! builder.add_memory_file("memory.dump")
//! builder.finalize()
//! ```
//!
//! ## Content-Defined Chunking (CDC)
//!
//! ```python
//! from strata import StrataBuilder
//!
//! # Enable CDC for better deduplication
//! builder = StrataBuilder(
//!     "snapshot.st",
//!     compression="zstd",
//!     cdc=True,
//!     min_chunk=16384,   # 16 KiB
//!     avg_chunk=65536,   # 64 KiB
//!     max_chunk=131072,  # 128 KiB
//! )
//! builder.add_disk_file("disk.img")
//! builder.finalize()
//! ```
//!
//! ## Overlay Merge (Thin Snapshots)
//!
//! ```python
//! from strata import StrataBuilder
//!
//! # Merge overlay changes into a new thin snapshot
//! builder = StrataBuilder("merged.st", compression="lz4")
//! builder.merge_overlay(
//!     base_path="base-snapshot.st",
//!     overlay_path="overlay.img",
//!     thin=True  # Create thin snapshot referencing base
//! )
//! builder.finalize()
//! ```
//!
//! ## Custom Metadata
//!
//! ```python
//! from strata import StrataBuilder
//! import json
//!
//! # Add custom metadata to snapshot
//! builder = StrataBuilder("snapshot.st")
//! metadata = {
//!     "vm_name": "production-db",
//!     "created_by": "backup-script",
//!     "tags": ["production", "database"]
//! }
//! builder.set_metadata(json.dumps(metadata).encode())
//! builder.add_disk_file("disk.img")
//! builder.finalize()
//! ```

use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use strata_common::constants::{BLOCK_OFFSET_PARENT, DEFAULT_ZSTD_LEVEL};
use strata_core::StrataFile;
use strata_core::algo::compression::{Compressor, lz4::Lz4Compressor, zstd::ZstdCompressor};
use strata_core::algo::dedup::{cdc::StreamChunker, dcam::DedupeParams};
use strata_core::api::stratafile::SnapshotStream;
use strata_core::format::{
    header::{CompressionType, FeatureFlags, StrataHeader},
    index::{BlockInfo, ENTRIES_PER_PAGE, IndexPage, MasterIndex, PageEntry},
    magic::{FORMAT_VERSION, HEADER_SIZE, MAGIC_BYTES},
};
use strata_core::store::local::FileBackend;

/// Result tuple returned by the thread-safe block processing closure.
/// Contains: (generated pages, new file offset, output file handle, number of blocks added, updated dedup map)
type BuilderResult = (Vec<PageEntry>, u64, File, u64, HashMap<[u8; 32], u64>);

type OverlayBuilderResult = (Vec<PageEntry>, u64, File, HashMap<[u8; 32], u64>);

struct FixedChunker<R> {
    reader: R,
    block_size: usize,
}

impl<R: Read> FixedChunker<R> {
    fn new(reader: R, block_size: usize) -> Self {
        Self { reader, block_size }
    }
}

impl<R: Read> Iterator for FixedChunker<R> {
    type Item = std::io::Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = vec![0u8; self.block_size];
        let mut pos = 0;
        while pos < self.block_size {
            match self.reader.read(&mut buf[pos..]) {
                Ok(0) => break,
                Ok(n) => pos += n,
                Err(e) => return Some(Err(e)),
            }
        }
        if pos == 0 {
            None
        } else {
            buf.truncate(pos);
            Some(Ok(buf))
        }
    }
}

#[pyclass(module = "strata.strata_loader")]
pub struct StrataBuilder {
    block_size: u32,
    compression: String,
    compression_level: Option<i32>,
    current_offset: u64,
    master: MasterIndex,
    writer: Option<File>,
    parent_path: Option<String>,
    disk_blocks_count: u64,
    memory_blocks_count: u64,
    dedup_enabled: bool,
    dedup_map: HashMap<[u8; 32], u64>,
    cdc_enabled: bool,
    min_chunk: u32,
    avg_chunk: u32,
    max_chunk: u32,
    metadata: Vec<u8>,
}

/// Python interface for building Strata snapshot files.
///
/// This class provides a low-level API for creating `.st` snapshot files from
/// disk images, memory dumps, or overlay files. It supports various compression
/// algorithms, deduplication, and content-defined chunking.
///
/// # Workflow
///
/// 1. Create a `StrataBuilder` instance with desired compression settings
/// 2. Add disk and/or memory files using `add_disk_file()` and `add_memory_file()`
/// 3. Optionally merge overlay files with `merge_overlay()`
/// 4. Call `finalize()` to write the index and header
///
/// # Performance Notes
///
/// - Deduplication is enabled by default and adds ~5-10% overhead
/// - Dictionary training can improve Zstd compression by 10-30%
/// - CDC chunking improves deduplication at the cost of metadata size
/// - Progress tracking is handled by the caller via callbacks (CLI layer)
#[pymethods]
impl StrataBuilder {
    /// Create a new snapshot builder.
    ///
    /// # Arguments
    ///
    /// - `output_path`: Path where the `.st` file will be written
    /// - `block_size`: Block size in bytes (default: 64 KiB)
    /// - `compression`: Compression algorithm: "lz4" or "zstd" (default: "lz4")
    /// - `compression_level`: Compression level for Zstd (default: 3)
    /// - `dedup`: Enable block-level deduplication (default: True)
    /// - `cdc`: Enable content-defined chunking for variable blocks (default: False)
    /// - `min_chunk`: Minimum chunk size for CDC in bytes (default: 16 KiB)
    /// - `avg_chunk`: Average chunk size for CDC in bytes (default: 64 KiB)
    /// - `max_chunk`: Maximum chunk size for CDC in bytes (default: 128 KiB)
    ///
    /// # Python Example
    ///
    /// ```python
    /// # Basic usage with defaults
    /// builder = StrataBuilder("output.st")
    ///
    /// # Custom compression settings
    /// builder = StrataBuilder(
    ///     "output.st",
    ///     compression="zstd",
    ///     compression_level=5,
    ///     block_size=131072  # 128 KiB blocks
    /// )
    ///
    /// # Enable CDC for better deduplication
    /// builder = StrataBuilder(
    ///     "output.st",
    ///     cdc=True,
    ///     min_chunk=32768,
    ///     avg_chunk=65536,
    ///     max_chunk=131072
    /// )
    /// ```
    #[new]
    #[pyo3(signature = (output_path, block_size=65536, compression="lz4", compression_level=None, dedup=true, cdc=false, min_chunk=16384, avg_chunk=65536, max_chunk=131072))]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        output_path: String,
        block_size: u32,
        compression: &str,
        compression_level: Option<i32>,
        dedup: bool,
        cdc: bool,
        min_chunk: u32,
        avg_chunk: u32,
        max_chunk: u32,
    ) -> PyResult<Self> {
        let path = PathBuf::from(output_path);
        let mut f = File::create(&path).map_err(|e| PyIOError::new_err(e.to_string()))?;

        // Write placeholder header
        f.write_all(&[0u8; HEADER_SIZE])
            .map_err(|e| PyIOError::new_err(e.to_string()))?;

        Ok(StrataBuilder {
            block_size,
            compression: compression.to_string(),
            compression_level,
            current_offset: HEADER_SIZE as u64,
            master: MasterIndex::default(),
            writer: Some(f),
            parent_path: None,
            disk_blocks_count: 0,
            memory_blocks_count: 0,
            dedup_enabled: dedup,
            dedup_map: HashMap::new(),
            cdc_enabled: cdc,
            min_chunk,
            avg_chunk,
            max_chunk,
            metadata: Vec::new(),
        })
    }

    /// Set custom metadata to be embedded in the snapshot.
    ///
    /// The metadata is stored as an opaque byte array in the snapshot and can be
    /// retrieved later using `StrataReader.metadata()`. Typical use cases include
    /// storing JSON-encoded configuration, tags, or provenance information.
    ///
    /// # Python Example
    ///
    /// ```python
    /// import json
    ///
    /// builder = StrataBuilder("snapshot.st")
    /// metadata = {"vm_name": "web-server", "version": "1.0"}
    /// builder.set_metadata(json.dumps(metadata).encode())
    /// ```
    pub fn set_metadata(&mut self, metadata: Vec<u8>) {
        self.metadata = metadata;
    }

    /// Get the current number of bytes written to the output file.
    ///
    /// This includes compressed block data and index pages but not the final
    /// header (which is written during `finalize()`).
    ///
    /// # Python Example
    ///
    /// ```python
    /// builder = StrataBuilder("snapshot.st")
    /// builder.add_disk_file("disk.img")
    /// print(f"Bytes written: {builder.get_bytes_written()}")
    /// ```
    pub fn get_bytes_written(&self) -> u64 {
        self.current_offset
    }

    /// Add a disk image file to the snapshot.
    ///
    /// This method reads the disk image, chunks it into blocks (fixed-size or CDC),
    /// compresses each block, performs deduplication, and writes the compressed data
    /// to the output file. The disk stream will be accessible via `SnapshotStream::Disk`
    /// when reading the snapshot.
    ///
    /// # Arguments
    ///
    /// - `path`: Path to the disk image file (raw format, qcow2, etc.)
    ///
    /// # Python Example
    ///
    /// ```python
    /// builder = StrataBuilder("snapshot.st")
    /// builder.add_disk_file("/path/to/disk.img")
    /// builder.finalize()
    /// ```
    pub fn add_disk_file<'py>(&mut self, py: Python<'py>, path: String) -> PyResult<()> {
        self.process_stream(py, path, true)
    }

    /// Add a memory dump file to the snapshot.
    ///
    /// This method reads the memory dump, chunks it, compresses, and deduplicates
    /// it just like `add_disk_file()`. The memory stream will be accessible via
    /// `SnapshotStream::Memory` when reading the snapshot, enabling VM live migration
    /// or checkpoint/restore scenarios.
    ///
    /// # Arguments
    ///
    /// - `path`: Path to the memory dump file
    ///
    /// # Python Example
    ///
    /// ```python
    /// # Create a full VM snapshot with disk and memory
    /// builder = StrataBuilder("vm-snapshot.st")
    /// builder.add_disk_file("disk.img")
    /// builder.add_memory_file("memory.dump")
    /// builder.finalize()
    /// ```
    pub fn add_memory_file<'py>(&mut self, py: Python<'py>, path: String) -> PyResult<()> {
        self.process_stream(py, path, false)
    }

    /// Merge an overlay file with a base snapshot to create a new snapshot.
    ///
    /// This method combines a base snapshot with an overlay file (created by FUSE mounts
    /// or VM overlays) to produce a new snapshot. Modified blocks are identified via the
    /// `.meta` file and merged with base snapshot data.
    ///
    /// # Thin vs. Thick Snapshots
    ///
    /// - **Thin snapshot** (`thin=True`): Unmodified blocks reference the parent snapshot
    ///   using the `BLOCK_OFFSET_PARENT` sentinel. The resulting snapshot requires the
    ///   parent to be accessible at read time.
    ///
    /// - **Thick snapshot** (`thin=False`): All blocks are copied into the new snapshot,
    ///   creating a standalone file that does not depend on the parent.
    ///
    /// # Arguments
    ///
    /// - `base_path`: Path to the base snapshot (`.st` file)
    /// - `overlay_path`: Path to the overlay file (raw image with modifications)
    /// - `thin`: If True, create a thin snapshot; if False, create a thick snapshot
    ///
    /// # Python Example
    ///
    /// ```python
    /// # Merge overlay changes into a thin snapshot
    /// builder = StrataBuilder("merged.st")
    /// builder.merge_overlay(
    ///     base_path="base-snapshot.st",
    ///     overlay_path="overlay.img",
    ///     thin=True
    /// )
    /// builder.finalize()
    ///
    /// # Create a standalone thick snapshot
    /// builder = StrataBuilder("standalone.st")
    /// builder.merge_overlay(
    ///     base_path="base-snapshot.st",
    ///     overlay_path="overlay.img",
    ///     thin=False
    /// )
    /// builder.finalize()
    /// ```
    #[pyo3(signature = (base_path, overlay_path, thin=false))]
    pub fn merge_overlay<'py>(
        &mut self,
        py: Python<'py>,
        base_path: String,
        overlay_path: String,
        thin: bool,
    ) -> PyResult<()> {
        let block_size = self.block_size as usize;
        let comp_str = self.compression.clone();

        let abs_base_path = std::fs::canonicalize(&base_path)
            .map_err(|e| PyIOError::new_err(format!("Failed to resolve base path: {}", e)))?;

        if thin {
            self.parent_path = Some(abs_base_path.to_string_lossy().to_string());
        }

        let abs_overlay_path = std::fs::canonicalize(&overlay_path)
            .map_err(|e| PyIOError::new_err(format!("Failed to resolve overlay path: {}", e)))?;

        let meta_path = abs_overlay_path.with_extension("meta");
        let mut modified_blocks = HashSet::new();

        if meta_path.exists() {
            let mut f = File::open(&meta_path).map_err(|e| PyIOError::new_err(e.to_string()))?;
            let mut buf = [0u8; 8];
            while f.read_exact(&mut buf).is_ok() {
                modified_blocks.insert(u64::from_le_bytes(buf));
            }
        }

        let backend = Arc::new(
            FileBackend::new(&abs_base_path).map_err(|e| PyIOError::new_err(e.to_string()))?,
        );
        let read_compressor = Box::new(Lz4Compressor::new());
        let base_snap = StrataFile::new(backend, read_compressor, None)
            .map_err(|e| PyIOError::new_err(e.to_string()))?;

        let base_size = base_snap.size(SnapshotStream::Disk);

        let mut ov_file =
            File::open(&abs_overlay_path).map_err(|e| PyIOError::new_err(e.to_string()))?;
        let ov_len = ov_file
            .metadata()
            .map_err(|e| PyIOError::new_err(e.to_string()))?
            .len();

        let final_size = std::cmp::max(base_size, ov_len);
        self.master.disk_size = final_size;

        let compression_level = self.compression_level;
        let write_compressor: Box<dyn Compressor> = match comp_str.as_str() {
            "zstd" => Box::new(ZstdCompressor::new(
                compression_level.unwrap_or(DEFAULT_ZSTD_LEVEL),
                None,
            )),
            _ => Box::new(Lz4Compressor::new()),
        };

        let mut out = self
            .writer
            .take()
            .ok_or_else(|| PyValueError::new_err("Writer closed"))?;
        let mut current_offset = self.current_offset;

        let dedup_enabled = self.dedup_enabled;
        let mut dedup_map = std::mem::take(&mut self.dedup_map);

        let (pages, new_offset, out_file, dedup_map) =
            py.allow_threads(move || -> PyResult<OverlayBuilderResult> {
                let mut pages = Vec::new();
                let mut page = IndexPage::default();
                let mut global_block_idx = 0;
                let mut page_start_block = 0;
                let mut page_start_logical = 0u64;
                let mut current_logical_pos = 0u64;

                let total_blocks = final_size.div_ceil(block_size as u64);
                let overlay_block_size = 4096;

                for i in 0..total_blocks {
                    let block_start = i * block_size as u64;
                    let mut len = block_size;
                    if block_start + len as u64 > final_size {
                        len = (final_size - block_start) as usize;
                    }

                    let start_ov_blk = block_start / overlay_block_size;
                    let end_ov_blk = (block_start + len as u64 - 1) / overlay_block_size;
                    let mut is_modified = false;
                    for ob in start_ov_blk..=end_ov_blk {
                        if modified_blocks.contains(&ob) {
                            is_modified = true;
                            break;
                        }
                    }

                    if thin && !is_modified && block_start < base_size {
                        page.blocks.push(BlockInfo {
                            offset: BLOCK_OFFSET_PARENT,
                            length: 0,
                            logical_len: len as u32,
                            checksum: 0,
                        });

                        global_block_idx += 1;
                        current_logical_pos += len as u64;

                        if page.blocks.len() >= ENTRIES_PER_PAGE {
                            let bytes = bincode::serialize(&page).unwrap();
                            let p_off = current_offset;
                            out.write_all(&bytes).unwrap();
                            current_offset += bytes.len() as u64;
                            pages.push(PageEntry {
                                offset: p_off,
                                length: bytes.len() as u32,
                                start_block: page_start_block,
                                start_logical: page_start_logical,
                            });
                            page = IndexPage::default();
                            page_start_block = global_block_idx;
                            page_start_logical = current_logical_pos;
                        }
                        continue;
                    }

                    let mut data = vec![0u8; len];

                    if is_modified {
                        if block_start < base_size {
                            if let Ok(base_data) =
                                base_snap.read_at(SnapshotStream::Disk, block_start, len)
                            {
                                data[..base_data.len()].copy_from_slice(&base_data);
                            }
                        }
                        for ob in start_ov_blk..=end_ov_blk {
                            if modified_blocks.contains(&ob) {
                                let chunk_start = ob * overlay_block_size;
                                let chunk_end =
                                    std::cmp::min(chunk_start + overlay_block_size, final_size);
                                let chunk_len = (chunk_end - chunk_start) as usize;

                                if chunk_start >= block_start
                                    && chunk_start < block_start + len as u64
                                {
                                    let rel_start = (chunk_start - block_start) as usize;
                                    ov_file
                                        .seek(SeekFrom::Start(chunk_start))
                                        .map_err(|e| PyIOError::new_err(e.to_string()))?;
                                    ov_file
                                        .read_exact(&mut data[rel_start..rel_start + chunk_len])
                                        .map_err(|e| PyIOError::new_err(e.to_string()))?;
                                }
                            }
                        }
                    } else if block_start < base_size {
                        if let Ok(base_data) =
                            base_snap.read_at(SnapshotStream::Disk, block_start, len)
                        {
                            data[..base_data.len()].copy_from_slice(&base_data);
                        }
                    }

                    if data.iter().all(|&b| b == 0) {
                        page.blocks.push(BlockInfo {
                            offset: 0,
                            length: 0,
                            logical_len: len as u32,
                            checksum: 0,
                        });
                    } else {
                        let compressed = write_compressor
                            .compress(&data)
                            .map_err(|e| PyValueError::new_err(e.to_string()))?;

                        let checksum = crc32fast::hash(&compressed);

                        let mut block_offset = current_offset;
                        let mut should_write = true;

                        if dedup_enabled {
                            let mut hasher = Sha256::new();
                            hasher.update(&compressed);
                            let hash: [u8; 32] = hasher.finalize().into();

                            if let Some(&existing_offset) = dedup_map.get(&hash) {
                                block_offset = existing_offset;
                                should_write = false;
                            } else {
                                dedup_map.insert(hash, current_offset);
                            }
                        }

                        if should_write {
                            out.write_all(&compressed)
                                .map_err(|e| PyIOError::new_err(e.to_string()))?;
                            current_offset += compressed.len() as u64;
                        }

                        page.blocks.push(BlockInfo {
                            offset: block_offset,
                            length: compressed.len() as u32,
                            logical_len: len as u32,
                            checksum,
                        });
                    }

                    global_block_idx += 1;
                    current_logical_pos += len as u64;

                    if page.blocks.len() >= ENTRIES_PER_PAGE {
                        let bytes = bincode::serialize(&page).unwrap();
                        let p_off = current_offset;
                        out.write_all(&bytes).unwrap();
                        current_offset += bytes.len() as u64;
                        pages.push(PageEntry {
                            offset: p_off,
                            length: bytes.len() as u32,
                            start_block: page_start_block,
                            start_logical: page_start_logical,
                        });
                        page = IndexPage::default();
                        page_start_block = global_block_idx;
                        page_start_logical = current_logical_pos;
                    }
                }

                if !page.blocks.is_empty() {
                    let bytes = bincode::serialize(&page).unwrap();
                    let p_off = current_offset;
                    out.write_all(&bytes).unwrap();
                    current_offset += bytes.len() as u64;
                    pages.push(PageEntry {
                        offset: p_off,
                        length: bytes.len() as u32,
                        start_block: page_start_block,
                        start_logical: page_start_logical,
                    });
                }

                Ok((pages, current_offset, out, dedup_map))
            })?;

        self.writer = Some(out_file);
        self.current_offset = new_offset;
        self.master.disk_pages = pages;
        self.dedup_map = dedup_map;

        Ok(())
    }

    /// Finalize the snapshot by writing the index, metadata, and header.
    ///
    /// This method must be called after adding all disk/memory files or merging overlays.
    /// It performs the following steps:
    ///
    /// 1. Serializes and writes the master index structure
    /// 2. Writes custom metadata (if set via `set_metadata()`)
    /// 3. Constructs and writes the snapshot header with all offsets
    /// 4. Closes the output file
    ///
    /// After calling `finalize()`, the builder cannot be reused.
    ///
    /// # Python Example
    ///
    /// ```python
    /// builder = StrataBuilder("snapshot.st")
    /// builder.add_disk_file("disk.img")
    /// builder.finalize()
    /// print("Snapshot created successfully")
    /// ```
    pub fn finalize(&mut self) -> PyResult<()> {
        let mut out = self
            .writer
            .take()
            .ok_or_else(|| PyValueError::new_err("Writer already closed"))?;

        // Write index first
        let index_offset = self.current_offset;
        let index_bytes =
            bincode::serialize(&self.master).map_err(|e| PyValueError::new_err(e.to_string()))?;
        out.write_all(&index_bytes)
            .map_err(|e| PyIOError::new_err(e.to_string()))?;
        self.current_offset += index_bytes.len() as u64;

        // Write metadata if present
        let (meta_offset, meta_len) = if !self.metadata.is_empty() {
            let offset = self.current_offset;
            out.write_all(&self.metadata)
                .map_err(|e| PyIOError::new_err(e.to_string()))?;
            self.current_offset += self.metadata.len() as u64;
            (Some(offset), Some(self.metadata.len() as u32))
        } else {
            (None, None)
        };

        let comp_type = match self.compression.as_str() {
            "zstd" => CompressionType::Zstd,
            _ => CompressionType::Lz4,
        };

        let header = StrataHeader {
            magic: *MAGIC_BYTES,
            version: FORMAT_VERSION,
            block_size: self.block_size,
            index_offset,
            parent_path: self.parent_path.clone(),
            dictionary_offset: None,
            dictionary_length: None,
            metadata_offset: meta_offset,
            metadata_length: meta_len,
            signature_offset: None,
            signature_length: None,
            encryption: None,
            compression: comp_type,
            features: FeatureFlags {
                has_disk: !self.master.disk_pages.is_empty(),
                has_memory: !self.master.memory_pages.is_empty(),
                variable_blocks: self.cdc_enabled,
            },
        };

        out.seek(SeekFrom::Start(0))
            .map_err(|e| PyIOError::new_err(e.to_string()))?;
        out.write_all(
            &bincode::serialize(&header).map_err(|e| PyValueError::new_err(e.to_string()))?,
        )
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

        Ok(())
    }
}

impl StrataBuilder {
    fn process_stream(&mut self, py: Python, path: String, is_disk: bool) -> PyResult<()> {
        let block_size = self.block_size as usize;
        let comp_str = self.compression.clone();
        let compression_level = self.compression_level;

        let f_in = File::open(&path).map_err(|e| PyIOError::new_err(e.to_string()))?;
        let len = f_in
            .metadata()
            .map_err(|e| PyIOError::new_err(e.to_string()))?
            .len();

        let start_logical_pos = if is_disk {
            self.master.disk_size
        } else {
            self.master.memory_size
        };
        let start_block_idx = if is_disk {
            self.disk_blocks_count
        } else {
            self.memory_blocks_count
        };

        let compressor: Box<dyn Compressor> = match comp_str.as_str() {
            "zstd" => Box::new(ZstdCompressor::new(
                compression_level.unwrap_or(DEFAULT_ZSTD_LEVEL),
                None,
            )),
            _ => Box::new(Lz4Compressor::new()),
        };

        let mut out = self
            .writer
            .take()
            .ok_or_else(|| PyValueError::new_err("Writer closed"))?;
        let mut current_offset = self.current_offset;

        let dedup_enabled = self.dedup_enabled;
        let mut dedup_map = std::mem::take(&mut self.dedup_map);

        let cdc_enabled = self.cdc_enabled;
        let cdc_params = if cdc_enabled {
            Some(DedupeParams {
                f: (self.avg_chunk as f64).log2() as u32,
                m: self.min_chunk,
                z: self.max_chunk,
                w: 48,
                v: 8,
            })
        } else {
            None
        };

        // Return: (pages, new_offset, out_file, added_blocks, dedup_map)
        let (pages, new_offset, out_file, added_blocks, dedup_map) =
            py.allow_threads(move || -> PyResult<BuilderResult> {
                let mut pages = Vec::new();
                let mut page = IndexPage::default();
                let mut global_block_idx = start_block_idx;
                let mut page_start_block = start_block_idx;
                let mut page_start_logical = start_logical_pos;
                let mut current_logical_pos = start_logical_pos;
                let mut blocks_added = 0u64;

                let chunker: Box<dyn Iterator<Item = std::io::Result<Vec<u8>>>> = if cdc_enabled {
                    Box::new(StreamChunker::new(f_in, cdc_params.unwrap()))
                } else {
                    Box::new(FixedChunker::new(f_in, block_size))
                };

                for chunk_res in chunker {
                    let chunk = chunk_res.map_err(|e| PyIOError::new_err(e.to_string()))?;
                    let n = chunk.len();

                    if chunk.iter().all(|&b| b == 0) {
                        page.blocks.push(BlockInfo {
                            offset: 0,
                            length: 0,
                            logical_len: n as u32,
                            checksum: 0,
                        });
                    } else {
                        let compressed = compressor
                            .compress(&chunk)
                            .map_err(|e| PyValueError::new_err(e.to_string()))?;

                        let checksum = crc32fast::hash(&compressed);
                        let mut block_offset = current_offset;
                        let mut should_write = true;

                        if dedup_enabled {
                            let mut hasher = Sha256::new();
                            hasher.update(&compressed);
                            let hash: [u8; 32] = hasher.finalize().into();

                            if let Some(&existing_offset) = dedup_map.get(&hash) {
                                block_offset = existing_offset;
                                should_write = false;
                            } else {
                                dedup_map.insert(hash, current_offset);
                            }
                        }

                        if should_write {
                            out.write_all(&compressed)
                                .map_err(|e| PyIOError::new_err(e.to_string()))?;
                            current_offset += compressed.len() as u64;
                        }

                        page.blocks.push(BlockInfo {
                            offset: block_offset,
                            length: compressed.len() as u32,
                            logical_len: n as u32,
                            checksum,
                        });
                    }

                    global_block_idx += 1;
                    blocks_added += 1;
                    current_logical_pos += n as u64;

                    if page.blocks.len() >= ENTRIES_PER_PAGE {
                        let bytes = bincode::serialize(&page).unwrap();
                        let p_off = current_offset;
                        out.write_all(&bytes).unwrap();
                        current_offset += bytes.len() as u64;

                        pages.push(PageEntry {
                            offset: p_off,
                            length: bytes.len() as u32,
                            start_block: page_start_block,
                            start_logical: page_start_logical,
                        });

                        page = IndexPage::default();
                        page_start_block = global_block_idx;
                        page_start_logical = current_logical_pos;
                    }
                }

                if !page.blocks.is_empty() {
                    let bytes = bincode::serialize(&page).unwrap();
                    let p_off = current_offset;
                    out.write_all(&bytes).unwrap();
                    current_offset += bytes.len() as u64;
                    pages.push(PageEntry {
                        offset: p_off,
                        length: bytes.len() as u32,
                        start_block: page_start_block,
                        start_logical: page_start_logical,
                    });
                }

                Ok((pages, current_offset, out, blocks_added, dedup_map))
            })?;

        self.writer = Some(out_file);
        self.current_offset = new_offset;
        self.dedup_map = dedup_map;

        if is_disk {
            self.master.disk_pages.extend(pages);
            self.master.disk_size += len;
            self.disk_blocks_count += added_blocks;
        } else {
            self.master.memory_pages.extend(pages);
            self.master.memory_size += len;
            self.memory_blocks_count += added_blocks;
        }

        Ok(())
    }
}
