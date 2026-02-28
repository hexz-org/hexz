//! Python utility functions for snapshot inspection, analysis, and operations.
//!
//! This module provides a collection of standalone Python functions for working with
//! Hexz snapshot files. These utilities cover inspection, cryptographic signing,
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
//! All functions raise Python exceptions from the `hexz.exceptions` hierarchy:
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
//! from hexz import inspect
//!
//! meta = inspect("snapshot.hxz")
//! print(f"Format version: {meta['version']}")
//! print(f"Compression: {meta['compression']}")
//! print(f"Encrypted: {meta['encrypted']}")
//! print(f"Primary size: {meta['primary_size']} bytes")
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
//! from hexz import analyze
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
//! from hexz import keygen, sign_image, verify_image
//!
//! # Generate keypair
//! priv_path, pub_path = keygen("keys/")
//! print(f"Generated: {priv_path}, {pub_path}")
//!
//! # Sign snapshot
//! sign_image("snapshot.hxz", "keys/private.key")
//! print("Snapshot signed successfully")
//!
//! # Verify signature
//! try:
//!     verify_image("snapshot.hxz", "keys/public.key")
//!     print("Signature valid!")
//! except ValueError as e:
//!     print(f"Verification failed: {e}")
//! ```
//!
//! ## Analyzing Overlay Changes
//!
//! ```python
//! from hexz import diff
//!
//! changes = diff("overlay.img")
//! print(f"Modified blocks: {changes['modified_blocks']}")
//! print(f"Estimated changed data: {changes['estimated_size']} bytes")
//! ```
//!
//! ## Live VM Snapshotting
//!
//! ```python
//! from hexz import snapshot_vm
//!
//! # Create snapshot of running VM via QMP
//! snapshot_vm(
//!     qmp_socket="/tmp/qemu-qmp.sock",
//!     base_path="base-snapshot.hxz",
//!     overlay_path="overlay.img",
//!     output_path="vm-snapshot.hxz"
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

use hexz_common::constants::{META_ENTRY_SIZE, OVERLAY_BLOCK_SIZE};
#[cfg(feature = "signing")]
use hexz_common::sign;
use hexz_core::algo::dedup::{cdc, dcam};
use hexz_core::format::version::{
    CURRENT_VERSION, MAX_SUPPORTED_VERSION, MIN_SUPPORTED_VERSION, check_version,
    compatibility_message,
};
use hexz_ops::inspect::inspect_snapshot;
#[cfg(feature = "signing")]
use hexz_ops::sign as core_sign;
use pyo3::exceptions::{PyIOError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use std::collections::HashMap;
use std::fs::File;
#[cfg(unix)]
use std::io::Write;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

#[cfg(unix)]
use super::builder::Builder;

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
/// from hexz import keygen
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
#[cfg(feature = "signing")]
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

/// Get the current Hexz format version number.
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
/// from hexz import get_format_version
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
/// from hexz import get_min_supported_version
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
/// from hexz import get_max_supported_version
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
/// - `primary_size` (int): Uncompressed primary stream size in bytes
/// - `secondary_size` (int): Uncompressed secondary stream size in bytes
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
/// from hexz import inspect
///
/// meta = inspect("snapshot.hxz")
///
/// # Format information
/// print(f"Version: {meta['version']}")
/// print(f"Compression: {meta['compression']}")
/// print(f"Block size: {meta['block_size']} bytes")
///
/// # Sizes and compression
/// print(f"Primary size: {meta['primary_size'] / (1024**3):.2f} GB")
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
pub fn inspect(py: Python<'_>, path: String) -> PyResult<PyObject> {
    let info = inspect_snapshot(&path).map_err(|e| PyIOError::new_err(e.to_string()))?;

    // Check version compatibility
    let compatibility = check_version(info.version);
    let is_compatible = compatibility.is_compatible();
    let compatibility_status = match compatibility {
        hexz_core::format::version::VersionCompatibility::Full => "full",
        hexz_core::format::version::VersionCompatibility::Degraded => "degraded",
        hexz_core::format::version::VersionCompatibility::Incompatible => "incompatible",
    };

    let dict = pyo3::types::PyDict::new(py);

    // Read user metadata if present (Python-specific JSON parsing)
    if let (Some(offset), Some(length)) = (info.metadata_offset, info.metadata_length) {
        if length > 0 {
            let mut f = File::open(&path).map_err(|e| PyIOError::new_err(e.to_string()))?;
            f.seek(SeekFrom::Start(offset))
                .map_err(|e| PyIOError::new_err(e.to_string()))?;
            let mut meta_bytes = vec![0u8; length as usize];
            f.read_exact(&mut meta_bytes)
                .map_err(|e| PyIOError::new_err(e.to_string()))?;

            if let Ok(json) = py.import("json") {
                let bytes_obj = pyo3::types::PyBytes::new(py, &meta_bytes);
                if let Ok(user_meta) = json.call_method1("loads", (bytes_obj,)) {
                    if let Ok(user_dict) = user_meta.downcast::<pyo3::types::PyDict>() {
                        for (k, v) in user_dict {
                            if let Ok(key) = k.extract::<String>() {
                                dict.set_item(key, v)?;
                            }
                        }
                    }
                }
            }
        }
    }

    let ratio = info.compression_ratio();

    dict.set_item("version", info.version)?;
    dict.set_item("current_version", CURRENT_VERSION)?;
    dict.set_item("min_supported_version", MIN_SUPPORTED_VERSION)?;
    dict.set_item("max_supported_version", MAX_SUPPORTED_VERSION)?;
    dict.set_item("is_compatible", is_compatible)?;
    dict.set_item("compatibility_status", compatibility_status)?;
    dict.set_item("compatibility_message", compatibility_message(info.version))?;
    dict.set_item("block_size", info.block_size)?;
    dict.set_item("compression", format!("{:?}", info.compression))?;
    dict.set_item("encrypted", info.encrypted)?;
    dict.set_item("parent_path", info.parent_paths)?;
    dict.set_item("primary_size", info.primary_size)?;
    dict.set_item("secondary_size", info.secondary_size)?;
    dict.set_item("file_size", info.file_size)?;
    dict.set_item("ratio", ratio)?;

    Ok(dict.into())
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
/// from hexz import analyze
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
        let mut file = File::open(&path_buf).map_err(|e| PyIOError::new_err(e.to_string()))?;
        let len = file
            .metadata()
            .map_err(|e| PyIOError::new_err(e.to_string()))?
            .len();

        let sample_size = 512 * 1024 * 1024;
        let read_len = std::cmp::min(len, sample_size);
        let mut buffer = vec![0u8; read_len as usize];

        file.read_exact(&mut buffer)
            .map_err(|e| PyIOError::new_err(e.to_string()))?;

        let baseline = dcam::DedupeParams::lbfs_baseline();
        let cdc_stats = cdc::analyze_stream(&buffer[..], &baseline)
            .map_err(|e| PyIOError::new_err(e.to_string()))?;

        // Use read_len for calculate_c (sampled data), full len for predict_ratio (whole file)
        let c = dcam::calculate_c(cdc_stats.unique_bytes, read_len, &baseline);
        let ratio = dcam::predict_ratio(len, c, &baseline);

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
/// from hexz import diff
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

    let count = len / META_ENTRY_SIZE as u64;
    let size = count * OVERLAY_BLOCK_SIZE;

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
/// from hexz import keygen, sign_image
///
/// # Generate keypair
/// priv_key, pub_key = keygen("keys/")
///
/// # Sign snapshot
/// sign_image("snapshot.hxz", priv_key)
/// print("Snapshot signed successfully")
/// ```
///
/// # Security Notes
///
/// - The signature covers the entire index structure (including block metadata)
/// - The signature does NOT cover block data (only structural integrity)
/// - Ed25519 signatures are 64 bytes
/// - Signatures are deterministic for the same key and message
#[cfg(feature = "signing")]
#[pyfunction]
pub fn sign_image(image_path: String, key_path: String) -> PyResult<()> {
    core_sign::sign_snapshot(&image_path, &key_path)
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
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
/// from hexz import verify_image
///
/// # Verify signature
/// try:
///     verify_image("snapshot.hxz", "keys/public.key")
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
#[cfg(feature = "signing")]
#[pyfunction]
pub fn verify_image(image_path: String, key_path: String) -> PyResult<()> {
    core_sign::verify_snapshot(&image_path, &key_path)
        .map_err(|e| PyValueError::new_err(format!("Verification failed: {}", e)))
}

#[cfg(unix)]
fn send_qmp(
    stream: &mut std::os::unix::net::UnixStream,
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

#[cfg(unix)]
fn read_qmp(stream: &mut std::os::unix::net::UnixStream) -> PyResult<serde_json::Value> {
    let mut all_data = Vec::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = stream
            .read(&mut buf)
            .map_err(|e| PyIOError::new_err(e.to_string()))?;
        if n == 0 {
            break;
        }
        all_data.extend_from_slice(&buf[..n]);
        // Check if we have a complete JSON line
        let s = String::from_utf8_lossy(&all_data);
        for line in s.lines() {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                if val.get("return").is_some() {
                    return Ok(val);
                }
            }
        }
        // If we got some data with a newline, we have complete responses
        if all_data.contains(&b'\n') {
            break;
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
/// from hexz import snapshot_vm
///
/// # Create live snapshot of running VM
/// snapshot_vm(
///     qmp_socket="/tmp/qemu-qmp.sock",
///     base_path="base-snapshot.hxz",
///     overlay_path="/var/lib/vm/overlay.qcow2",
///     output_path="vm-checkpoint.hxz"
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
/// - Total downtime: secondary_size_gb / 0.5-2 GB/s (rough estimate)
///
/// # Safety
///
/// - VM is automatically resumed even if snapshot creation fails
/// - Uses thick merge by default (independent snapshot)
/// - Memory dump is written to secure temporary file
#[cfg(unix)]
#[pyfunction]
pub fn snapshot_vm(
    py: Python<'_>,
    qmp_socket: String,
    base_path: String,
    overlay_path: String,
    output_path: String,
) -> PyResult<()> {
    let socket_path = PathBuf::from(qmp_socket);

    let mut stream = std::os::unix::net::UnixStream::connect(&socket_path)
        .map_err(|e| PyIOError::new_err(format!("Failed to connect to QMP: {}", e)))?;

    let _ = read_qmp(&mut stream)?;
    let _ = send_qmp(&mut stream, "qmp_capabilities", None)?;
    let _ = send_qmp(&mut stream, "stop", None)?;

    let mem_dump = tempfile::NamedTempFile::new().map_err(|e| PyIOError::new_err(e.to_string()))?;
    let mem_path = mem_dump.path().to_string_lossy().to_string();

    let migrate_cmd = format!("exec:cat > '{}'", mem_path.replace('\'', "'\\''"));
    let args = serde_json::json!({ "uri": migrate_cmd });
    let _ = send_qmp(&mut stream, "migrate", Some(args))?;

    let migrate_start = std::time::Instant::now();
    let migrate_timeout = std::time::Duration::from_secs(600); // 10 minute timeout
    loop {
        if migrate_start.elapsed() > migrate_timeout {
            let _ = send_qmp(&mut stream, "cont", None);
            return Err(PyRuntimeError::new_err(
                "Memory dump timed out after 600 seconds",
            ));
        }
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

    let build_result = (|| -> PyResult<()> {
        let mut builder = Builder::new(
            output_path,
            65536,
            "lz4",
            None,
            true,
            None, // min_chunk (LBFS baseline)
            None, // avg_chunk (LBFS baseline)
            None, // max_chunk (LBFS baseline)
            None, // parent
            true, // cdc=true for VM disk images
            0,    // num_workers (auto)
        )?;
        builder.merge_overlay(py, base_path, overlay_path, false)?;
        builder.add_memory_file(py, mem_path)?;
        builder.finalize()?;
        Ok(())
    })();

    // Always resume VM, even if snapshot creation failed
    let _ = send_qmp(&mut stream, "cont", None);

    // Now propagate any build error
    build_result?;

    Ok(())
}
