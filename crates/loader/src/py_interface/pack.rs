//! High-level Python packing function for creating Strata snapshots.
//!
//! This module provides the `pack()` function, a convenient high-level API for creating
//! Strata snapshot files from disk images and memory dumps. It wraps the core
//! `strata_core::ops::pack::pack_snapshot` function with Python bindings.
//!
//! # Overview
//!
//! The `pack()` function is the recommended way to create snapshots from Python when you
//! need a simple, high-level interface. For more control over the packing process
//! (e.g., streaming inputs, overlay merging, custom metadata), use `StrataBuilder` instead.
//!
//! # Key Features
//!
//! - **Simple API**: Single function call to create snapshots
//! - **Multiple Inputs**: Pack disk images, memory dumps, or both
//! - **Compression**: Support for LZ4 (fast) and Zstandard (high ratio)
//! - **Encryption**: Optional AES-256-GCM encryption with password
//! - **CDC Chunking**: Content-defined chunking for better deduplication
//! - **GIL Release**: Releases Python's GIL during packing for parallelism
//!
//! # Usage Patterns
//!
//! ## Basic Snapshot Creation
//!
//! ```python
//! from strata import pack
//!
//! # Pack a disk image with default settings (LZ4, 64 KiB blocks)
//! pack(output="snapshot.st", disk="disk.img")
//! ```
//!
//! ## VM Snapshot with Disk and Memory
//!
//! ```python
//! # Pack both disk and memory for full VM checkpoint
//! pack(
//!     output="vm-checkpoint.st",
//!     disk="disk.img",
//!     memory="memory.dump"
//! )
//! ```
//!
//! ## High Compression with Zstandard
//!
//! ```python
//! # Use Zstandard for maximum compression ratio
//! pack(
//!     output="snapshot.st",
//!     disk="disk.img",
//!     compression="zstd",
//!     block_size=131072  # 128 KiB blocks for better compression
//! )
//! ```
//!
//! ## Encrypted Snapshots
//!
//! ```python
//! # Create encrypted snapshot (AES-256-GCM)
//! pack(
//!     output="encrypted.st",
//!     disk="disk.img",
//!     encrypt=True,
//!     password="my-secure-password"
//! )
//! ```
//!
//! ## Content-Defined Chunking for Deduplication
//!
//! ```python
//! # Enable CDC for better deduplication across similar files
//! pack(
//!     output="snapshot.st",
//!     disk="disk.img",
//!     cdc=True,
//!     min_chunk=16384,   # 16 KiB minimum
//!     avg_chunk=65536,   # 64 KiB average
//!     max_chunk=131072   # 128 KiB maximum
//! )
//! ```
//!
//! # Comparison with StrataBuilder
//!
//! Use `pack()` when:
//! - You have simple inputs (disk/memory files)
//! - You want a one-line solution
//! - You don't need custom metadata or overlay merging
//!
//! Use `StrataBuilder` when:
//! - You need to merge overlays
//! - You want to add custom metadata
//! - You need fine-grained control over the packing process
//! - You want progress callbacks (via CLI layer)
//!
//! # Performance Considerations
//!
//! - **Compression Speed**: LZ4 is ~5x faster than Zstandard but achieves ~30% lower
//!   compression ratio. Use LZ4 for fast backups, Zstandard for archival storage.
//!
//! - **Block Size**: Larger blocks (128 KiB+) improve compression ratio but reduce
//!   random access granularity and increase decompression overhead for small reads.
//!
//! - **CDC Overhead**: Content-defined chunking adds ~10-15% overhead but can significantly
//!   improve deduplication on similar snapshots (incremental backups).
//!
//! - **Encryption**: Adds ~5-10% overhead. Encrypted snapshots cannot be deduplicated
//!   across runs due to random IVs.

use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use std::path::PathBuf;
use strata_core::ops::pack::{PackConfig, pack_snapshot};

/// Pack disk and/or memory images into a Strata snapshot file.
///
/// This high-level function creates a compressed snapshot from disk images and/or memory
/// dumps with optional encryption, deduplication, and content-defined chunking. It releases
/// the Python GIL during packing, allowing other threads to execute concurrently.
///
/// # Arguments
///
/// - `output` (str): Output path for the `.st` snapshot file
///
/// - `disk` (str, optional): Path to disk image file to pack. Supports raw images, qcow2,
///   and other formats that can be read as raw bytes.
///
/// - `memory` (str, optional): Path to memory dump file to pack (for VM checkpoints)
///
/// - `compression` (str, default="lz4"): Compression algorithm:
///   - `"lz4"`: Fast compression with moderate ratio (~2-3x)
///   - `"zstd"`: Slower compression with high ratio (~4-8x)
///
/// - `block_size` (int, default=65536): Block size in bytes. Must be a power of 2.
///   Larger blocks improve compression but reduce random access granularity.
///   Typical values: 65536 (64 KiB), 131072 (128 KiB)
///
/// - `encrypt` (bool, default=False): Enable AES-256-GCM encryption
///
/// - `password` (str, optional): Encryption password. Required if `encrypt=True`.
///
/// - `cdc` (bool, default=False): Enable content-defined chunking for variable-sized blocks.
///   Improves deduplication across similar snapshots at cost of metadata size.
///
/// - `min_chunk` (int, default=16384): Minimum CDC chunk size in bytes (16 KiB)
///
/// - `avg_chunk` (int, default=65536): Average CDC chunk size in bytes (64 KiB)
///
/// - `max_chunk` (int, default=131072): Maximum CDC chunk size in bytes (128 KiB)
///
/// # Raises
///
/// - `ValueError`: Invalid parameters (e.g., `encrypt=True` without password)
/// - `IOError`: Failed to read input files or write output snapshot
///
/// # Python Examples
///
/// ```python
/// from strata import pack
///
/// # Basic disk snapshot with defaults
/// pack(output="snapshot.st", disk="disk.img")
///
/// # Full VM checkpoint with disk and memory
/// pack(
///     output="vm.st",
///     disk="disk.img",
///     memory="memory.dump"
/// )
///
/// # High compression with Zstandard
/// pack(
///     output="compressed.st",
///     disk="disk.img",
///     compression="zstd",
///     block_size=131072  # 128 KiB blocks
/// )
///
/// # Encrypted snapshot
/// pack(
///     output="secure.st",
///     disk="disk.img",
///     encrypt=True,
///     password="my-password"
/// )
///
/// # CDC for deduplication
/// pack(
///     output="deduped.st",
///     disk="disk.img",
///     cdc=True,
///     min_chunk=32768,
///     avg_chunk=65536,
///     max_chunk=131072
/// )
/// ```
///
/// # Performance Notes
///
/// - The GIL is released during packing, allowing other Python threads to run
/// - LZ4 compression is ~5x faster than Zstandard but achieves lower ratios
/// - Larger block sizes improve compression but reduce random access performance
/// - CDC adds overhead but significantly improves deduplication
#[allow(clippy::too_many_arguments)]
#[pyfunction]
#[pyo3(signature = (
    output,
    disk=None,
    memory=None,
    compression="lz4",
    block_size=65536,
    encrypt=false,
    password=None,
    cdc=false,
    min_chunk=16384,
    avg_chunk=65536,
    max_chunk=131072
))]
pub fn pack(
    py: Python<'_>,
    output: String,
    disk: Option<String>,
    memory: Option<String>,
    compression: &str,
    block_size: u32,
    encrypt: bool,
    password: Option<String>,
    cdc: bool,
    min_chunk: u32,
    avg_chunk: u32,
    max_chunk: u32,
) -> PyResult<()> {
    if encrypt && password.is_none() {
        return Err(PyValueError::new_err(
            "Password is required when encryption is enabled",
        ));
    }

    let config = PackConfig {
        disk: disk.map(PathBuf::from),
        memory: memory.map(PathBuf::from),
        output: PathBuf::from(output),
        compression: compression.to_string(),
        encrypt,
        password,
        train_dict: false,
        block_size,
        cdc_enabled: cdc,
        min_chunk,
        avg_chunk,
        max_chunk,
    };

    // Release the GIL during the potentially long-running pack operation
    py.allow_threads(move || {
        pack_snapshot(config, None::<fn(u64, u64)>).map_err(|e| PyIOError::new_err(e.to_string()))
    })
}
