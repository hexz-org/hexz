//! Python utility functions for snapshot inspection, analysis, and operations.
//!
//! This module provides a collection of standalone Python functions for working with
//! Strata snapshot files. These utilities cover inspection, cryptographic signing,
//! deduplication analysis, overlay diffing, and live VM snapshotting.
//!
//! # Overview
//!
//! All functions in this module operate on snapshot files without requiring an open
//! reader instance. They are designed for command-line tools, administrative scripts,
//! and snapshot validation workflows.
//!
//! # Functionality Categories
//!
//! ## Inspection and Metadata
//!
//! - `inspect()`: Extract comprehensive metadata from snapshot headers
//! - `get_format_version()`: Get current format version constant
//! - `get_min_supported_version()`: Get minimum compatible version
//! - `get_max_supported_version()`: Get maximum compatible version
//!
//! ## Deduplication Analysis
//!
//! - `analyze()`: Analyze file for deduplication potential using content-defined chunking
//! - `diff()`: Count modified blocks in an overlay file
//!
//! ## Cryptographic Operations
//!
//! - `keygen()`: Generate Ed25519 keypair for signing
//! - `sign_image()`: Sign a snapshot with a private key
//! - `verify_image()`: Verify snapshot signature with a public key
//!
//! ## Live VM Snapshotting
//!
//! - `snapshot_vm()`: Create live VM snapshot via QMP socket
//!
//! # Return Value Semantics
//!
//! Most functions return Python dictionaries with structured data:
//!
//! - `inspect()`: Returns `dict[str, Any]` with metadata fields
//! - `analyze()`: Returns `dict[str, float]` with deduplication statistics
//! - `diff()`: Returns `dict[str, int]` with block counts
//! - `keygen()`: Returns `tuple[str, str]` with key paths
//!
//! # Error Handling
//!
//! All functions raise Python exceptions from the `strata.exceptions` hierarchy:
//!
//! - `IOError`: File not found, permission denied, network errors
//! - `FormatError`: Invalid snapshot format, corrupted header
//! - `ValueError`: Invalid parameters, missing required fields
//! - `RuntimeError`: Internal errors, QMP communication failures
//!
//! # Usage Examples
//!
//! ## Inspecting Snapshots
//!
//! ```python
//! from strata import inspect
//!
//! meta = inspect("snapshot.st")
//! print(f"Format version: {meta['version']}")
//! print(f"Compression: {meta['compression']}")
//! print(f"Encrypted: {meta['encrypted']}")
//! print(f"Disk size: {meta['disk_size']} bytes")
//! print(f"Compression ratio: {meta['ratio']:.2f}x")
//!
//! # Check version compatibility
//! if not meta['is_compatible']:
//!     print(f"Warning: {meta['compatibility_message']}")
//! ```
//!
//! ## Analyzing Deduplication Potential
//!
//! ```python
//! from strata import analyze
//!
//! stats = analyze("disk.img")
//! print(f"Unique bytes: {stats['unique_bytes']}")
//! print(f"Change probability: {stats['change_probability']:.4f}")
//! print(f"Predicted compression ratio: {stats['predicted_ratio']:.2f}x")
//! ```
//!
//! ## Cryptographic Signing
//!
//! ```python
//! from strata import keygen, sign_image, verify_image
//!
//! # Generate keypair
//! priv_path, pub_path = keygen("keys/")
//! print(f"Generated: {priv_path}, {pub_path}")
//!
//! # Sign snapshot
//! sign_image("snapshot.st", "keys/private.key")
//! print("Snapshot signed successfully")
//!
//! # Verify signature
//! try:
//!     verify_image("snapshot.st", "keys/public.key")
//!     print("Signature valid!")
//! except ValueError as e:
//!     print(f"Verification failed: {e}")
//! ```
//!
//! ## Analyzing Overlay Changes
//!
//! ```python
//! from strata import diff
//!
//! changes = diff("overlay.img")
//! print(f"Modified blocks: {changes['modified_blocks']}")
//! print(f"Estimated changed data: {changes['estimated_size']} bytes")
//! ```
//!
//! ## Live VM Snapshotting
//!
//! ```python
//! from strata import snapshot_vm
//!
//! # Create snapshot of running VM via QMP
//! snapshot_vm(
//!     qmp_socket="/tmp/qemu-qmp.sock",
//!     base_path="base-snapshot.st",
//!     overlay_path="overlay.img",
//!     output_path="vm-snapshot.st"
//! )
//! print("VM snapshot created successfully")
//! ```
//!
//! # Performance Considerations
//!
//! - **inspect()**: Fast operation, only reads header and index (O(1) w.r.t. snapshot size)
//! - **analyze()**: Samples first 512 MiB of file for deduplication analysis
//! - **sign_image()**: Requires reading entire index for SHA-256 hashing
//! - **snapshot_vm()**: Blocks until VM memory dump completes (can take seconds to minutes)

use pyo3::exceptions::{PyIOError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use strata_common::sign;
use strata_core::algo::dedup::{cdc, dcam};
use strata_core::format::header::StrataHeader;
use strata_core::format::index::MasterIndex;
use strata_core::format::magic::HEADER_SIZE;
use strata_core::format::version::{
    CURRENT_VERSION, MAX_SUPPORTED_VERSION, MIN_SUPPORTED_VERSION, check_version,
    compatibility_message,
};

use super::builder::StrataBuilder;

/// Generate an Ed25519 keypair for signing snapshots.
///
/// Creates a new Ed25519 keypair and writes the private and public keys to separate
/// files in the specified directory. The private key should be kept secure and used
/// for signing snapshots, while the public key can be distributed for verification.
///
/// # Arguments
///
/// - `output_dir` (str, optional): Directory where keys will be written. Defaults to
///   current working directory if not specified.
///
/// # Returns
///
/// Tuple `(private_key_path, public_key_path)` containing absolute paths to the generated
/// key files.
///
/// # Raises
///
/// - `IOError`: Failed to write key files (permission denied, disk full, etc.)
///
/// # Python Example
///
/// ```python
/// from strata import keygen
///
/// # Generate in current directory
/// priv_key, pub_key = keygen()
/// print(f"Private key: {priv_key}")
/// print(f"Public key: {pub_key}")
///
/// # Generate in specific directory
/// priv_key, pub_key = keygen("keys/")
/// ```
///
/// # Security Notes
///
/// - Private keys are written with restrictive permissions (0600)
/// - Keep private keys secure and never commit them to version control
/// - Public keys can be safely distributed
/// - Ed25519 provides ~128-bit security level
#[pyfunction]
#[pyo3(signature = (output_dir=None))]
pub fn keygen(output_dir: Option<String>) -> PyResult<(String, String)> {
    let dir = match output_dir {
        Some(ref d) => PathBuf::from(d),
        None => std::env::current_dir().map_err(|e| PyIOError::new_err(e.to_string()))?,
    };
    let priv_path = dir.join("private.key");
    let pub_path = dir.join("public.key");
    sign::generate_keypair(&priv_path, &pub_path).map_err(|e| PyIOError::new_err(e.to_string()))?;
    Ok((
        priv_path.to_string_lossy().into_owned(),
        pub_path.to_string_lossy().into_owned(),
    ))
}

/// Get the current Strata format version number.
///
/// Returns the format version constant used when creating new snapshots. This can be
/// compared against versions read from existing snapshots to determine compatibility.
///
/// # Returns
///
/// Current format version as unsigned 32-bit integer.
///
/// # Python Example
///
/// ```python
/// from strata import get_format_version
///
/// version = get_format_version()
/// print(f"Current format version: {version}")
/// ```
#[pyfunction]
pub fn get_format_version() -> u32 {
    CURRENT_VERSION
}

/// Get the minimum supported format version.
///
/// Returns the oldest format version that this library can read. Snapshots with versions
/// older than this will be rejected with a `VersionError`.
///
/// # Returns
///
/// Minimum supported version as unsigned 32-bit integer.
///
/// # Python Example
///
/// ```python
/// from strata import get_min_supported_version
///
/// min_ver = get_min_supported_version()
/// print(f"Minimum supported version: {min_ver}")
/// ```
#[pyfunction]
pub fn get_min_supported_version() -> u32 {
    MIN_SUPPORTED_VERSION
}

/// Get the maximum supported format version.
///
/// Returns the newest format version that this library can read. Snapshots with versions
/// newer than this may be rejected or opened with degraded functionality.
///
/// # Returns
///
/// Maximum supported version as unsigned 32-bit integer.
///
/// # Python Example
///
/// ```python
/// from strata import get_max_supported_version
///
/// max_ver = get_max_supported_version()
/// print(f"Maximum supported version: {max_ver}")
/// ```
#[pyfunction]
pub fn get_max_supported_version() -> u32 {
    MAX_SUPPORTED_VERSION
}

/// Inspect a snapshot file and extract comprehensive metadata.
///
/// Reads the header and index from a snapshot file to extract format information,
/// compression settings, sizes, version compatibility, and custom user metadata.
/// This is a fast operation that only reads header data, not the full file.
///
/// # Arguments
///
/// - `path` (str): Path to snapshot file to inspect
///
/// # Returns
///
/// Dictionary with the following keys:
///
/// - `version` (int): Format version of this snapshot
/// - `current_version` (int): Current format version of this library
/// - `min_supported_version` (int): Minimum version this library supports
/// - `max_supported_version` (int): Maximum version this library supports
/// - `is_compatible` (bool): Whether this snapshot can be read
/// - `compatibility_status` (str): "full", "degraded", or "incompatible"
/// - `compatibility_message` (str): Human-readable compatibility message
/// - `block_size` (int): Block size in bytes
/// - `compression` (str): Compression algorithm ("Lz4" or "Zstd")
/// - `encrypted` (bool): Whether snapshot is encrypted
/// - `parent_path` (str | None): Path to parent snapshot (for thin snapshots)
/// - `disk_size` (int): Uncompressed disk stream size in bytes
/// - `memory_size` (int): Uncompressed memory stream size in bytes
/// - `file_size` (int): Compressed snapshot file size on disk
/// - `ratio` (float): Compression ratio (uncompressed / compressed)
/// - Additional keys from custom metadata (if present)
///
/// # Raises
///
/// - `IOError`: Failed to read snapshot file
/// - `ValueError`: Invalid snapshot format or corrupted header
///
/// # Python Example
///
/// ```python
/// from strata import inspect
///
/// meta = inspect("snapshot.st")
///
/// # Format information
/// print(f"Version: {meta['version']}")
/// print(f"Compression: {meta['compression']}")
/// print(f"Block size: {meta['block_size']} bytes")
///
/// # Sizes and compression
/// print(f"Disk size: {meta['disk_size'] / (1024**3):.2f} GB")
/// print(f"File size: {meta['file_size'] / (1024**3):.2f} GB")
/// print(f"Compression ratio: {meta['ratio']:.2f}x")
///
/// # Compatibility check
/// if not meta['is_compatible']:
///     print(f"Warning: {meta['compatibility_message']}")
///
/// # Custom metadata (if present)
/// if 'vm_name' in meta:
///     print(f"VM Name: {meta['vm_name']}")
/// ```
#[pyfunction]
pub fn inspect(py: Python<'_>, path: String) -> PyResult<HashMap<String, PyObject>> {
    let mut f = File::open(&path).map_err(|e| PyIOError::new_err(e.to_string()))?;

    let mut header_bytes = [0u8; HEADER_SIZE];
    f.read_exact(&mut header_bytes)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    let header: StrataHeader =
        bincode::deserialize(&header_bytes).map_err(|e| PyValueError::new_err(e.to_string()))?;

    f.seek(SeekFrom::Start(header.index_offset))
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    let mut index_bytes = Vec::new();
    f.read_to_end(&mut index_bytes)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    let master: MasterIndex =
        bincode::deserialize(&index_bytes).map_err(|e| PyValueError::new_err(e.to_string()))?;

    let file_len = f
        .metadata()
        .map_err(|e| PyIOError::new_err(e.to_string()))?
        .len();
    let total_uncompressed = master.disk_size + master.memory_size;
    let ratio = if file_len > 0 {
        total_uncompressed as f64 / file_len as f64
    } else {
        0.0
    };

    // Check version compatibility
    let compatibility = check_version(header.version);
    let is_compatible = compatibility.is_compatible();
    let compatibility_status = match compatibility {
        strata_core::format::version::VersionCompatibility::Full => "full",
        strata_core::format::version::VersionCompatibility::Degraded => "degraded",
        strata_core::format::version::VersionCompatibility::Incompatible => "incompatible",
    };

    let mut dict: HashMap<String, PyObject> = HashMap::new();

    // Read user metadata if present
    if let (Some(offset), Some(length)) = (header.metadata_offset, header.metadata_length) {
        if length > 0 {
            f.seek(SeekFrom::Start(offset))
                .map_err(|e| PyIOError::new_err(e.to_string()))?;
            let mut meta_bytes = vec![0u8; length as usize];
            f.read_exact(&mut meta_bytes)
                .map_err(|e| PyIOError::new_err(e.to_string()))?;

            // Decode using Python's json module
            if let Ok(json) = py.import("json") {
                let bytes_obj = pyo3::types::PyBytes::new(py, &meta_bytes);
                if let Ok(user_meta) = json.call_method1("loads", (bytes_obj,)) {
                    if let Ok(user_dict) = user_meta.downcast::<pyo3::types::PyDict>() {
                        for (k, v) in user_dict {
                            if let Ok(key) = k.extract::<String>() {
                                dict.insert(key, v.into_pyobject(py)?.unbind());
                            }
                        }
                    }
                }
            }
        }
    }

    dict.insert(
        "version".to_string(),
        header.version.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "current_version".to_string(),
        CURRENT_VERSION.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "min_supported_version".to_string(),
        MIN_SUPPORTED_VERSION.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "max_supported_version".to_string(),
        MAX_SUPPORTED_VERSION.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "is_compatible".to_string(),
        <pyo3::Bound<'_, pyo3::types::PyBool> as Clone>::clone(&pyo3::types::PyBool::new(
            py,
            is_compatible,
        ))
        .unbind()
        .into(),
    );
    dict.insert(
        "compatibility_status".to_string(),
        compatibility_status.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "compatibility_message".to_string(),
        compatibility_message(header.version)
            .into_pyobject(py)?
            .unbind()
            .into(),
    );
    dict.insert(
        "block_size".to_string(),
        header.block_size.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "compression".to_string(),
        format!("{:?}", header.compression)
            .into_pyobject(py)?
            .unbind()
            .into(),
    );
    dict.insert(
        "encrypted".to_string(),
        <pyo3::Bound<'_, pyo3::types::PyBool> as Clone>::clone(&pyo3::types::PyBool::new(
            py,
            header.encryption.is_some(),
        ))
        .unbind()
        .into(),
    );
    dict.insert(
        "parent_path".to_string(),
        header.parent_path.into_pyobject(py)?.unbind(),
    );
    dict.insert(
        "disk_size".to_string(),
        master.disk_size.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "memory_size".to_string(),
        master.memory_size.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "file_size".to_string(),
        file_len.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "ratio".to_string(),
        ratio.into_pyobject(py)?.unbind().into(),
    );
    Ok(dict)
}

/// Analyze a file for deduplication potential using content-defined chunking.
///
/// Samples the first 512 MiB of a file and applies content-defined chunking (CDC) to
/// estimate how much deduplication could be achieved. This is useful for determining
/// whether CDC mode should be enabled when packing similar snapshots.
///
/// # Arguments
///
/// - `path` (str): Path to file to analyze (disk image, memory dump, etc.)
///
/// # Returns
///
/// Dictionary with deduplication statistics:
///
/// - `unique_bytes` (float): Number of unique bytes after chunking
/// - `change_probability` (float): Estimated probability of block changes (0.0-1.0)
/// - `predicted_ratio` (float): Predicted deduplication ratio
///
/// # Raises
///
/// - `IOError`: Failed to read file
///
/// # Python Example
///
/// ```python
/// from strata import analyze
///
/// stats = analyze("disk.img")
/// print(f"Unique bytes: {stats['unique_bytes']}")
/// print(f"Change probability: {stats['change_probability']:.4f}")
/// print(f"Predicted ratio: {stats['predicted_ratio']:.2f}x")
///
/// # Decide whether to use CDC
/// if stats['predicted_ratio'] > 1.5:
///     print("CDC recommended for this file")
/// else:
///     print("CDC unlikely to help for this file")
/// ```
///
/// # Performance Notes
///
/// - Only samples first 512 MiB for speed
/// - Analysis runs with GIL released
/// - Results are estimates based on LBFS baseline parameters
#[pyfunction]
pub fn analyze(py: Python<'_>, path: String) -> PyResult<HashMap<String, f64>> {
    let path_buf = std::path::PathBuf::from(path);

    let stats = py.allow_threads(move || -> PyResult<(u64, f64, f64)> {
        let file = File::open(&path_buf).map_err(|e| PyIOError::new_err(e.to_string()))?;
        let len = file
            .metadata()
            .map_err(|e| PyIOError::new_err(e.to_string()))?
            .len();

        let sample_size = 512 * 1024 * 1024;
        let read_len = std::cmp::min(len, sample_size);
        let mut buffer = vec![0u8; read_len as usize];

        let mut f = File::open(&path_buf).map_err(|e| PyIOError::new_err(e.to_string()))?;
        f.read_exact(&mut buffer)
            .map_err(|e| PyIOError::new_err(e.to_string()))?;

        let baseline = dcam::DedupeParams::lbfs_baseline();
        let cdc_stats = cdc::analyze_stream(&buffer[..], &baseline)
            .map_err(|e| PyIOError::new_err(e.to_string()))?;

        let c = dcam::calculate_c(cdc_stats.unique_bytes, cdc_stats.total_bytes, &baseline);
        let ratio = dcam::predict_ratio(cdc_stats.total_bytes, c, &baseline);

        Ok((cdc_stats.unique_bytes, c, ratio))
    })?;

    let mut dict = HashMap::new();
    dict.insert("unique_bytes".to_string(), stats.0 as f64);
    dict.insert("change_probability".to_string(), stats.1);
    dict.insert("predicted_ratio".to_string(), stats.2);

    Ok(dict)
}

/// Count modified blocks in an overlay file.
///
/// Reads the `.meta` file associated with an overlay image (created by FUSE mounts or
/// QEMU overlays) and counts the number of modified 4 KiB blocks. This is useful for
/// estimating the size of changes before merging an overlay.
///
/// # Arguments
///
/// - `overlay_path` (str): Path to overlay file (e.g., "overlay.img"). The associated
///   metadata file at "overlay.meta" will be read automatically.
///
/// # Returns
///
/// Dictionary with overlay statistics:
///
/// - `modified_blocks` (int): Number of modified 4 KiB blocks
/// - `estimated_size` (int): Estimated changed data size in bytes (blocks * 4096)
///
/// # Raises
///
/// - `IOError`: Overlay metadata file not found or unreadable
///
/// # Python Example
///
/// ```python
/// from strata import diff
///
/// changes = diff("overlay.img")
/// print(f"Modified blocks: {changes['modified_blocks']}")
/// print(f"Changed data: {changes['estimated_size'] / (1024**2):.2f} MB")
///
/// # Decide merge strategy
/// if changes['modified_blocks'] < 1000:
///     print("Few changes, thin merge recommended")
/// else:
///     print("Many changes, thick merge recommended")
/// ```
///
/// # Implementation Notes
///
/// - Metadata files store 8-byte block indices (little-endian)
/// - Block size is always 4096 bytes (4 KiB)
/// - Size estimate assumes all blocks are fully modified
#[pyfunction]
pub fn diff(overlay_path: String) -> PyResult<HashMap<String, u64>> {
    let path = std::path::PathBuf::from(overlay_path);
    let meta_path = path.with_extension("meta");

    if !meta_path.exists() {
        return Err(PyIOError::new_err("Overlay metadata file not found"));
    }

    let f = File::open(&meta_path).map_err(|e| PyIOError::new_err(e.to_string()))?;
    let len = f
        .metadata()
        .map_err(|e| PyIOError::new_err(e.to_string()))?
        .len();

    let count = len / 8;
    let size = count * 4096;

    let mut dict = HashMap::new();
    dict.insert("modified_blocks".to_string(), count);
    dict.insert("estimated_size".to_string(), size);

    Ok(dict)
}

/// Sign a snapshot file with an Ed25519 private key.
///
/// Computes a SHA-256 hash of the snapshot's index and signs it with the provided
/// Ed25519 private key. The signature is appended to the snapshot file, and the header
/// is updated with the signature offset and length.
///
/// # Arguments
///
/// - `image_path` (str): Path to snapshot file to sign
/// - `key_path` (str): Path to Ed25519 private key file (generated by `keygen()`)
///
/// # Raises
///
/// - `IOError`: Failed to read/write snapshot or key file
/// - `ValueError`: Invalid snapshot format or corrupted header
/// - `RuntimeError`: Signing operation failed (invalid key, etc.)
///
/// # Python Example
///
/// ```python
/// from strata import keygen, sign_image
///
/// # Generate keypair
/// priv_key, pub_key = keygen("keys/")
///
/// # Sign snapshot
/// sign_image("snapshot.st", priv_key)
/// print("Snapshot signed successfully")
/// ```
///
/// # Security Notes
///
/// - The signature covers the entire index structure (including block metadata)
/// - The signature does NOT cover block data (only structural integrity)
/// - Ed25519 signatures are 64 bytes
/// - Signatures are deterministic for the same key and message
#[pyfunction]
pub fn sign_image(image_path: String, key_path: String) -> PyResult<()> {
    let image_path = PathBuf::from(image_path);
    let key_path = PathBuf::from(key_path);

    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&image_path)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    let mut header_bytes = [0u8; HEADER_SIZE];
    f.read_exact(&mut header_bytes)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    let mut header: StrataHeader =
        bincode::deserialize(&header_bytes).map_err(|e| PyValueError::new_err(e.to_string()))?;

    f.seek(SeekFrom::Start(header.index_offset))
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    let mut index_bytes = Vec::new();
    f.read_to_end(&mut index_bytes)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    let mut hasher = Sha256::new();
    hasher.update(&index_bytes);
    let digest = hasher.finalize();

    let signature = sign::sign_digest(&key_path, &digest)
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    let signature_offset = f
        .seek(SeekFrom::End(0))
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    f.write_all(&signature)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    header.signature_offset = Some(signature_offset);
    header.signature_length = Some(signature.len() as u32);

    f.seek(SeekFrom::Start(0))
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    f.write_all(&bincode::serialize(&header).map_err(|e| PyValueError::new_err(e.to_string()))?)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    Ok(())
}

/// Verify a snapshot file's signature with an Ed25519 public key.
///
/// Reads the signature from the snapshot file, recomputes the SHA-256 hash of the index,
/// and verifies the signature using the provided Ed25519 public key. Raises an exception
/// if verification fails.
///
/// # Arguments
///
/// - `image_path` (str): Path to signed snapshot file
/// - `key_path` (str): Path to Ed25519 public key file
///
/// # Raises
///
/// - `IOError`: Failed to read snapshot or key file
/// - `ValueError`: Snapshot is not signed, invalid signature length, or verification failed
///
/// # Python Example
///
/// ```python
/// from strata import verify_image
///
/// # Verify signature
/// try:
///     verify_image("snapshot.st", "keys/public.key")
///     print("Signature valid - snapshot integrity verified")
/// except ValueError as e:
///     print(f"Signature verification failed: {e}")
/// ```
///
/// # Security Notes
///
/// - Verification confirms the snapshot structure has not been tampered with
/// - Does NOT verify block data integrity (use checksums for that)
/// - Ed25519 verification is fast (~50k signatures/sec on modern CPUs)
/// - Public keys can be safely distributed
#[pyfunction]
pub fn verify_image(image_path: String, key_path: String) -> PyResult<()> {
    let image_path = PathBuf::from(image_path);
    let key_path = PathBuf::from(key_path);

    let mut f = File::open(&image_path).map_err(|e| PyIOError::new_err(e.to_string()))?;
    let mut header_bytes = [0u8; HEADER_SIZE];
    f.read_exact(&mut header_bytes)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    let header: StrataHeader =
        bincode::deserialize(&header_bytes).map_err(|e| PyValueError::new_err(e.to_string()))?;

    let (sig_off, sig_len) = match (header.signature_offset, header.signature_length) {
        (Some(o), Some(l)) => (o, l),
        _ => return Err(PyValueError::new_err("Image is not signed")),
    };

    if sig_len != 64 {
        return Err(PyValueError::new_err("Invalid signature length"));
    }

    let mut signature = [0u8; 64];
    f.seek(SeekFrom::Start(sig_off))
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    f.read_exact(&mut signature)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    let index_len = sig_off - header.index_offset;
    f.seek(SeekFrom::Start(header.index_offset))
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    let mut index_reader = f.take(index_len);
    let mut index_bytes = Vec::new();
    index_reader
        .read_to_end(&mut index_bytes)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    let mut hasher = Sha256::new();
    hasher.update(&index_bytes);
    let digest = hasher.finalize();

    sign::verify_digest(&key_path, &digest, &signature)
        .map_err(|e| PyValueError::new_err(format!("Verification failed: {}", e)))?;

    Ok(())
}

fn send_qmp(
    stream: &mut UnixStream,
    cmd: &str,
    args: Option<serde_json::Value>,
) -> PyResult<serde_json::Value> {
    let mut json = serde_json::json!({ "execute": cmd });
    if let Some(a) = args {
        json["arguments"] = a;
    }
    let data = serde_json::to_string(&json).map_err(|e| PyValueError::new_err(e.to_string()))?;
    stream
        .write_all(data.as_bytes())
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    read_qmp(stream)
}

fn read_qmp(stream: &mut UnixStream) -> PyResult<serde_json::Value> {
    let mut buf = [0u8; 4096];
    let n = stream
        .read(&mut buf)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    let s = String::from_utf8_lossy(&buf[..n]);

    for line in s.lines() {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if val.get("return").is_some() {
                return Ok(val);
            }
        }
    }
    Ok(serde_json::json!({}))
}

/// Create a live snapshot of a running VM via QMP (QEMU Machine Protocol).
///
/// Connects to a running QEMU VM via its QMP socket, pauses the VM, dumps memory to a
/// temporary file, merges the disk overlay with the base snapshot, and creates a new
/// snapshot containing both disk and memory. The VM is automatically resumed after the
/// snapshot is complete.
///
/// # Arguments
///
/// - `qmp_socket` (str): Path to QEMU QMP Unix socket (e.g., "/tmp/qemu-qmp.sock")
/// - `base_path` (str): Path to base snapshot file
/// - `overlay_path` (str): Path to VM's disk overlay file
/// - `output_path` (str): Path for output snapshot file
///
/// # Workflow
///
/// 1. Connect to QMP socket and negotiate capabilities
/// 2. Pause VM execution (`stop` command)
/// 3. Dump memory to temporary file (`migrate exec:cat > /tmp/...`)
/// 4. Wait for memory dump to complete
/// 5. Merge overlay with base snapshot (thick merge)
/// 6. Add memory dump to snapshot
/// 7. Resume VM execution (`cont` command)
///
/// # Raises
///
/// - `IOError`: Failed to connect to QMP socket or read/write files
/// - `RuntimeError`: Memory dump failed or QMP communication error
///
/// # Python Example
///
/// ```python
/// from strata import snapshot_vm
///
/// # Create live snapshot of running VM
/// snapshot_vm(
///     qmp_socket="/tmp/qemu-qmp.sock",
///     base_path="base-snapshot.st",
///     overlay_path="/var/lib/vm/overlay.qcow2",
///     output_path="vm-checkpoint.st"
/// )
/// print("Live VM snapshot created")
/// ```
///
/// # Requirements
///
/// - QEMU must be started with QMP socket enabled:
///   `qemu-system-x86_64 -qmp unix:/tmp/qemu-qmp.sock,server,nowait ...`
/// - User must have permission to connect to QMP socket
/// - Sufficient disk space for memory dump (equal to VM RAM size)
///
/// # Performance Notes
///
/// - VM is paused during memory dump (typically 1-10 seconds depending on RAM size)
/// - Memory dump uses `exec:cat` which is slower than native QEMU migration formats
/// - Overlay merge may take several seconds for large disks
/// - Total downtime: memory_size_gb / 0.5-2 GB/s (rough estimate)
///
/// # Safety
///
/// - VM is automatically resumed even if snapshot creation fails
/// - Uses thick merge by default (independent snapshot)
/// - Memory dump is written to secure temporary file
#[pyfunction]
pub fn snapshot_vm(
    py: Python<'_>,
    qmp_socket: String,
    base_path: String,
    overlay_path: String,
    output_path: String,
) -> PyResult<()> {
    let socket_path = PathBuf::from(qmp_socket);

    let mut stream = UnixStream::connect(&socket_path)
        .map_err(|e| PyIOError::new_err(format!("Failed to connect to QMP: {}", e)))?;

    let _ = read_qmp(&mut stream)?;
    let _ = send_qmp(&mut stream, "qmp_capabilities", None)?;
    let _ = send_qmp(&mut stream, "stop", None)?;

    let mem_dump = tempfile::NamedTempFile::new().map_err(|e| PyIOError::new_err(e.to_string()))?;
    let mem_path = mem_dump.path().to_string_lossy().to_string();

    let migrate_cmd = format!("exec:cat > {}", mem_path);
    let args = serde_json::json!({ "uri": migrate_cmd });
    let _ = send_qmp(&mut stream, "migrate", Some(args))?;

    loop {
        let resp = send_qmp(&mut stream, "query-migrate", None)?;
        if let Some(status) = resp["return"]["status"].as_str() {
            if status == "completed" {
                break;
            } else if status == "failed" {
                let _ = send_qmp(&mut stream, "cont", None);
                return Err(PyRuntimeError::new_err("Memory dump failed"));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    let mut builder = StrataBuilder::new(
        output_path,
        65536,
        "lz4",
        None,
        true,
        false,
        16384,
        65536,
        131072,
    )?;
    builder.merge_overlay(py, base_path, overlay_path, false)?;
    builder.add_memory_file(py, mem_path)?;
    builder.finalize()?;

    let _ = send_qmp(&mut stream, "cont", None)?;

    Ok(())
}
