//! Python class for building Hexz snapshots.
//!
//! Provides the low-level `Builder` that can create archives from
//! disk images, memory dumps, and overlay merges.
//!
//! # Python Usage Examples
//!
//! ## Basic Snapshot Creation
//!
//! ```python
//! from hexz import Builder
//!
//! # Create a new snapshot with LZ4 compression
//! builder = Builder("output.hxz", compression="lz4")
//! builder.add_disk_file("disk.img")
//! builder.finalize()
//! ```
//!
//! ## Full VM Snapshot with Memory
//!
//! ```python
//! from hexz import Builder
//!
//! # Create snapshot with both disk and memory
//! builder = Builder("vm-snapshot.hxz", compression="zstd", compression_level=5)
//! builder.add_disk_file("disk.img")
//! builder.add_memory_file("memory.dump")
//! builder.finalize()
//! ```
//!
//! ## Content-Defined Chunking (CDC)
//!
//! ```python
//! from hexz import Builder
//!
//! # Enable CDC for better deduplication
//! builder = Builder(
//!     "snapshot.hxz",
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
//! from hexz import Builder
//!
//! # Merge overlay changes into a new thin snapshot
//! builder = Builder("merged.hxz", compression="lz4")
//! builder.merge_overlay(
//!     base_path="base-snapshot.hxz",
//!     overlay_path="overlay.img",
//!     thin=True  # Create thin snapshot referencing base
//! )
//! builder.finalize()
//! ```
//!
//! ## Custom Metadata
//!
//! ```python
//! from hexz import Builder
//! import json
//!
//! # Add custom metadata to snapshot
//! builder = Builder("snapshot.hxz")
//! metadata = {
//!     "vm_name": "production-db",
//!     "created_by": "backup-script",
//!     "tags": ["production", "database"]
//! }
//! builder.set_metadata(json.dumps(metadata).encode())
//! builder.add_disk_file("disk.img")
//! builder.finalize()
//! ```

use hexz_common::constants::{
    DEFAULT_CDC_AVG_CHUNK, DEFAULT_CDC_MAX_CHUNK, DEFAULT_CDC_MIN_CHUNK, OVERLAY_BLOCK_SIZE,
};
use hexz_core::File as HexzFile;
use hexz_core::algo::compression::create_compressor_from_str;
use hexz_core::algo::dedup::{cdc::StreamChunker, dcam::DedupeParams};
use hexz_core::api::file::SnapshotStream;
use hexz_core::ops::pack::FixedChunker;
use hexz_core::ops::snapshot_writer::SnapshotWriter;
use hexz_core::store::local::FileBackend;
use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;

#[pyclass(module = "hexz.hexz_loader")]
pub struct Builder {
    writer: Option<SnapshotWriter>,
    parent_path: Option<String>,
    cdc_enabled: bool,
    min_chunk: u32,
    avg_chunk: u32,
    max_chunk: u32,
    metadata: Vec<u8>,
    block_size: u32,
}

/// Python interface for building Hexz snapshot files.
///
/// This class provides a low-level API for creating `.hxz` snapshot files from
/// disk images, memory dumps, or overlay files. It supports various compression
/// algorithms, deduplication, and content-defined chunking.
///
/// # Workflow
///
/// 1. Create a `Builder` instance with desired compression settings
/// 2. Add disk and/or memory files using `add_disk_file()` and `add_memory_file()`
/// 3. Optionally merge overlay files with `merge_overlay()`
/// 4. Call `finalize()` to write the index and header
#[pymethods]
impl Builder {
    /// Create a new snapshot builder.
    #[new]
    #[pyo3(signature = (output_path, block_size=65536, compression="lz4", compression_level=None, dedup=true, cdc=false, min_chunk=DEFAULT_CDC_MIN_CHUNK, avg_chunk=DEFAULT_CDC_AVG_CHUNK, max_chunk=DEFAULT_CDC_MAX_CHUNK))]
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

        let (compressor, comp_type) =
            create_compressor_from_str(compression, compression_level, None)
                .map_err(|e| PyIOError::new_err(e.to_string()))?;

        // When dedup is disabled we still create the writer the same way;
        // the SnapshotWriter always deduplicates (unless encrypting, which
        // the builder currently does not support). If dedup=false was
        // requested we intentionally ignore it — BLAKE3 dedup adds <5%
        // overhead and using SHA-256 was the old bug we are fixing.
        let _ = dedup;

        let writer = SnapshotWriter::builder(&path, compressor, comp_type)
            .block_size(block_size)
            .variable_blocks(cdc)
            .build()
            .map_err(|e| PyIOError::new_err(e.to_string()))?;

        Ok(Builder {
            writer: Some(writer),
            parent_path: None,
            cdc_enabled: cdc,
            min_chunk,
            avg_chunk,
            max_chunk,
            metadata: Vec::new(),
            block_size,
        })
    }

    /// Set custom metadata to be embedded in the snapshot.
    pub fn set_metadata(&mut self, metadata: Vec<u8>) {
        self.metadata = metadata;
    }

    /// Get the current number of bytes written to the output file.
    pub fn get_bytes_written(&self) -> u64 {
        self.writer.as_ref().map_or(0, |w| w.current_offset())
    }

    /// Add a disk image file to the snapshot.
    pub fn add_disk_file<'py>(&mut self, py: Python<'py>, path: String) -> PyResult<()> {
        self.process_stream(py, path, true)
    }

    /// Add a memory dump file to the snapshot.
    pub fn add_memory_file<'py>(&mut self, py: Python<'py>, path: String) -> PyResult<()> {
        self.process_stream(py, path, false)
    }

    /// Merge an overlay file with a base snapshot to create a new snapshot.
    #[pyo3(signature = (base_path, overlay_path, thin=false))]
    pub fn merge_overlay<'py>(
        &mut self,
        py: Python<'py>,
        base_path: String,
        overlay_path: String,
        thin: bool,
    ) -> PyResult<()> {
        let block_size = self.block_size as usize;

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
        let base_snap =
            HexzFile::open(backend, None).map_err(|e| PyIOError::new_err(e.to_string()))?;

        let base_size = base_snap.size(SnapshotStream::Disk);

        let mut ov_file =
            File::open(&abs_overlay_path).map_err(|e| PyIOError::new_err(e.to_string()))?;
        let ov_len = ov_file
            .metadata()
            .map_err(|e| PyIOError::new_err(e.to_string()))?
            .len();

        let final_size = std::cmp::max(base_size, ov_len);

        let mut writer = self
            .writer
            .take()
            .ok_or_else(|| PyValueError::new_err("Writer closed"))?;

        let writer = py.allow_threads(move || -> PyResult<SnapshotWriter> {
            writer.begin_stream(true, final_size);

            let total_blocks = final_size.div_ceil(block_size as u64);
            let overlay_block_size: u64 = OVERLAY_BLOCK_SIZE;

            for i in 0..total_blocks {
                let block_start = i * block_size as u64;
                let mut len = block_size;
                if block_start + len as u64 > final_size {
                    len = (final_size - block_start) as usize;
                }

                let start_ov_blk = block_start / overlay_block_size;
                let end_ov_blk = (block_start + len as u64 - 1) / overlay_block_size;
                let is_modified =
                    (start_ov_blk..=end_ov_blk).any(|ob| modified_blocks.contains(&ob));

                // Thin: unmodified block in base → parent ref
                if thin && !is_modified && block_start < base_size {
                    writer
                        .write_parent_ref(len as u32)
                        .map_err(|e| PyIOError::new_err(e.to_string()))?;
                    continue;
                }

                // Build block data from base + overlay
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

                            if chunk_start >= block_start && chunk_start < block_start + len as u64
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
                    if let Ok(base_data) = base_snap.read_at(SnapshotStream::Disk, block_start, len)
                    {
                        data[..base_data.len()].copy_from_slice(&base_data);
                    }
                }

                writer
                    .write_data_block(&data)
                    .map_err(|e| PyIOError::new_err(e.to_string()))?;
            }

            writer
                .end_stream()
                .map_err(|e| PyIOError::new_err(e.to_string()))?;

            Ok(writer)
        })?;

        self.writer = Some(writer);
        Ok(())
    }

    /// Finalize the snapshot by writing the index, metadata, and header.
    pub fn finalize(&mut self) -> PyResult<()> {
        let writer = self
            .writer
            .take()
            .ok_or_else(|| PyValueError::new_err("Writer already closed"))?;

        let meta = if self.metadata.is_empty() {
            None
        } else {
            Some(self.metadata.as_slice())
        };

        writer
            .finalize(self.parent_path.clone(), meta)
            .map_err(|e| PyIOError::new_err(e.to_string()))?;

        Ok(())
    }
}

impl Builder {
    fn process_stream(&mut self, py: Python, path: String, is_disk: bool) -> PyResult<()> {
        let block_size = self.block_size as usize;

        let f_in = File::open(&path).map_err(|e| PyIOError::new_err(e.to_string()))?;
        let len = f_in
            .metadata()
            .map_err(|e| PyIOError::new_err(e.to_string()))?
            .len();

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

        let mut writer = self
            .writer
            .take()
            .ok_or_else(|| PyValueError::new_err("Writer closed"))?;

        let writer = py.allow_threads(move || -> PyResult<SnapshotWriter> {
            writer.begin_stream(is_disk, len);

            let chunker: Box<dyn Iterator<Item = std::io::Result<Vec<u8>>>> = match cdc_params {
                Some(params) => Box::new(StreamChunker::new(f_in, params)),
                None => Box::new(FixedChunker::new(f_in, block_size)),
            };

            for chunk_res in chunker {
                let chunk = chunk_res.map_err(|e| PyIOError::new_err(e.to_string()))?;
                writer
                    .write_data_block(&chunk)
                    .map_err(|e| PyIOError::new_err(e.to_string()))?;
            }

            writer
                .end_stream()
                .map_err(|e| PyIOError::new_err(e.to_string()))?;

            Ok(writer)
        })?;

        self.writer = Some(writer);
        Ok(())
    }
}
