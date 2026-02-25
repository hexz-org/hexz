//! Synchronous Python snapshot reader with file-like interface.
//!
//! This module provides the `Reader` class, a synchronous Python interface for reading
//! Hexz snapshot files. It implements Python's standard file-like protocol (`read`, `seek`,
//! `tell`, etc.) with extensions for high-performance random access and zero-copy buffer
//! operations.
//!
//! # Overview
//!
//! `Reader` wraps the high-performance `hexz_core::File` engine with a Python-
//! friendly API. It supports both cursor-based sequential reading and explicit offset-based
//! random access, making it suitable for streaming ML datasets, virtual machine disk access,
//! and general-purpose compressed file reading.
//!
//! # Key Features
//!
//! - **Dual Access Modes**: Cursor-based sequential reads or explicit offset random access
//! - **Zero-Copy Reads**: Direct memory mapping into NumPy arrays and buffer protocol objects
//! - **GIL Release**: All I/O operations release the Global Interpreter Lock for parallelism
//! - **Thread Safety**: Cursor state protected by `Mutex` allows safe concurrent access
//! - **Pickle Support**: Full serialization support for multiprocessing workflows
//! - **Context Manager**: Implements `__enter__`/`__exit__` for resource management
//!
//! # Cursor-Based vs. Absolute Reads
//!
//! ## Cursor-Based Reading (Sequential Access)
//!
//! When `offset` is **not** provided, reads start from the current cursor position and
//! advance the cursor by the number of bytes read. This is optimal for streaming datasets
//! where you read sequentially from beginning to end.
//!
//! ```python
//! reader = Reader("data.hxz")
//! chunk1 = reader.read(1024)  # reads bytes 0-1023, cursor moves to 1024
//! chunk2 = reader.read(1024)  # reads bytes 1024-2047, cursor moves to 2048
//! ```
//!
//! ## Absolute Reading (Random Access)
//!
//! When `offset` is **provided**, reads start from that absolute position without modifying
//! the cursor. This enables random access patterns common in ML training with shuffled indices.
//!
//! ```python
//! reader = Reader("data.hxz")
//! chunk = reader.read(1024, offset=4096)  # reads bytes 4096-5119, cursor unchanged
//! pos = reader.tell()  # still 0
//! ```
//!
//! # Zero-Copy Buffer Operations
//!
//! For maximum performance when working with NumPy arrays or other buffer-supporting types,
//! use `read(buffer=...)` or `readinto()` to avoid allocating intermediate byte arrays:
//!
//! ```python
//! import numpy as np
//!
//! reader = Reader("data.hxz")
//! buffer = np.zeros(4096, dtype=np.uint8)
//!
//! # Zero-copy read directly into NumPy array
//! bytes_read = reader.read(buffer=buffer, offset=0)
//! # buffer now contains decompressed data without intermediate copies
//! ```
//!
//! # Performance Notes
//!
//! - **Prefetching**: Enable background block prefetching with `prefetch_count` parameter
//!   when opening snapshots with sequential access patterns (ML dataset streaming).
//!
//! - **Cache Sizing**: Configure `cache_capacity_bytes` based on working set size. The
//!   default is conservative (~4MB); increase for better random access performance.
//!
//! - **GIL Release**: All I/O operations release Python's GIL, allowing multiple threads
//!   to perform I/O concurrently. For CPU-bound processing, use one reader per thread.
//!
//! - **Decompression**: Decompression happens on-demand per block. Blocks are typically
//!   64 KiB, so reading 1 byte still decompresses the entire block (but results are cached).
//!
//! # Thread Safety
//!
//! `Reader` is thread-safe: the cursor is protected by a `Mutex`, allowing multiple
//! threads to safely share a single reader instance. However, for best performance in
//! multi-threaded scenarios, create one reader per thread to avoid cursor contention.
//!
//! # Integration Examples
//!
//! ## PyTorch DataLoader Integration
//!
//! ```python
//! import torch
//! from torch.utils.data import Dataset, DataLoader
//! from hexz import Reader
//! import numpy as np
//!
//! class Dataset(Dataset):
//!     def __init__(self, path, item_size):
//!         self.reader = Reader(path, prefetch_count=16)
//!         self.item_size = item_size
//!         self.length = self.reader.size() // item_size
//!
//!     def __len__(self):
//!         return self.length
//!
//!     def __getitem__(self, idx):
//!         offset = idx * self.item_size
//!         buffer = np.zeros(self.item_size, dtype=np.uint8)
//!         self.reader.read(buffer=buffer, offset=offset)
//!         return torch.from_numpy(buffer)
//!
//! dataset = Dataset("training.hxz", item_size=1024)
//! loader = DataLoader(dataset, batch_size=32, num_workers=4)
//! ```
//!
//! ## S3 Remote Reading
//!
//! ```python
//! # Read directly from S3 (credentials from environment)
//! reader = Reader(
//!     "s3://my-bucket/dataset.hxz",
//!     s3_region="us-west-2",
//!     cache_capacity_bytes=1024*1024*1024  # 1 GB cache for remote reads
//! )
//! data = reader.read(4096)
//! ```
//!
//! ## HTTP Streaming
//!
//! ```python
//! # Read from HTTP with range request support
//! reader = Reader("https://example.com/data.hxz")
//! data = reader.read(1024, offset=0)  # Range: bytes=0-1023
//! ```
//!
//! # Pickle Support for Multiprocessing
//!
//! `Reader` implements `__getstate__` and `__setstate__` to support pickling,
//! enabling safe usage with Python's `multiprocessing` module:
//!
//! ```python
//! import multiprocessing as mp
//! from hexz import Reader
//!
//! def worker(reader, offset, size):
//!     data = reader.read(size, offset=offset)
//!     return len(data)
//!
//! if __name__ == "__main__":
//!     reader = Reader("data.hxz")
//!     with mp.Pool(4) as pool:
//!         results = pool.starmap(worker, [
//!             (reader, 0, 1024),
//!             (reader, 1024, 1024),
//!             (reader, 2048, 1024),
//!             (reader, 3072, 1024),
//!         ])
//! ```

use hexz_core::File;
use hexz_core::api::file::SnapshotStream;
use pyo3::exceptions::{PyIOError, PyOSError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::sync::{Arc, Mutex};

use crate::engine::{self, OpenConfig};
use crate::tensor;

/// Synchronous Python reader for Hexz snapshot files.
///
/// `Reader` provides a file-like interface for reading compressed snapshot files
/// with support for both sequential cursor-based access and random absolute offset access.
/// It implements Python's standard I/O protocol and extends it with zero-copy buffer
/// operations for high-performance data loading.
///
/// # Constructor Parameters
///
/// - `path` (str): Path to snapshot file. Supports:
///   - Local paths: `/path/to/file.hxz`
///   - HTTP(S) URLs: `https://example.com/data.hxz`
///   - S3 URIs: `s3://bucket/key.hxz`
///
/// - `s3_region` (str, optional): AWS region for S3 URIs (e.g., "us-west-2")
///
/// - `endpoint_url` (str, optional): Custom S3 endpoint for MinIO, Ceph, etc.
///
/// - `allow_restricted` (bool, default=False): Allow connections to private IP ranges.
///   Enable only in trusted environments.
///
/// - `prefetch_count` (int, default=0): Number of blocks to prefetch in background.
///   Set to 8-32 for sequential access patterns (ML training). Disabled by default.
///
/// - `cache_capacity_bytes` (int, optional): Block cache size in bytes. Default is
///   conservative (~4MB). Increase for random access workloads.
///
/// # Attributes
///
/// The following properties and methods are available on instances:
///
/// - `size()`: Returns total uncompressed size in bytes
/// - `tell()`: Returns current cursor position
/// - `readable()`: Returns `True` (always readable)
/// - `seekable()`: Returns `True` (always seekable)
/// - `writable()`: Returns `False` (read-only)
///
/// # Methods
///
/// See individual method documentation for detailed usage.
///
/// # Python Example
///
/// ```python
/// from hexz import Reader
/// import numpy as np
///
/// # Open snapshot with prefetching enabled
/// reader = Reader("data.hxz", prefetch_count=16)
///
/// # Get total size
/// total = reader.size()
/// print(f"Snapshot size: {total} bytes")
///
/// # Sequential reading (cursor-based)
/// chunk1 = reader.read(4096)  # reads from cursor, advances it
/// chunk2 = reader.read(4096)  # continues from where we left off
///
/// # Random access (absolute offset)
/// data = reader.read(1024, offset=8192)  # cursor unchanged
///
/// # Zero-copy into NumPy array
/// buffer = np.zeros(4096, dtype=np.uint8)
/// bytes_read = reader.read(buffer=buffer, offset=0)
///
/// # Seek operations
/// reader.seek(0)  # rewind to start
/// reader.seek(-4096, 2)  # seek to 4096 bytes before end
///
/// # Context manager support
/// with Reader("data.hxz") as reader:
///     data = reader.read(1024)
/// # automatically closed
/// ```
#[pyclass(module = "hexz.hexz_loader")]
pub struct Reader {
    pub(crate) inner: Arc<File>,
    path: String,
    cursor: Mutex<u64>,
}

#[pymethods]
impl Reader {
    /// Create a new Reader instance.
    ///
    /// Opens a snapshot file for reading with optional configuration for caching,
    /// prefetching, and remote storage access.
    ///
    /// # Arguments
    ///
    /// - `path`: Path to snapshot file (local, HTTP, or S3)
    /// - `s3_region`: AWS region for S3 URLs (e.g., "us-west-2")
    /// - `endpoint_url`: Custom S3 endpoint URL for MinIO, Ceph, etc.
    /// - `allow_restricted`: Allow connections to private/internal IP addresses
    /// - `prefetch_count`: Number of blocks to prefetch (0 = disabled, 8-32 for sequential)
    /// - `cache_capacity_bytes`: Block cache size in bytes (None = default ~4MB)
    ///
    /// # Returns
    ///
    /// New `Reader` instance positioned at offset 0.
    ///
    /// # Raises
    ///
    /// - `IOError`: File not found, permission denied, or network error
    /// - `FormatError`: Invalid snapshot format or corrupted header
    /// - `VersionError`: Unsupported format version
    ///
    /// # Python Example
    ///
    /// ```python
    /// # Local file with defaults
    /// reader = Reader("data.hxz")
    ///
    /// # S3 with region and large cache
    /// reader = Reader(
    ///     "s3://bucket/data.hxz",
    ///     s3_region="us-west-2",
    ///     cache_capacity_bytes=1024*1024*1024  # 1 GB
    /// )
    ///
    /// # HTTP with prefetching for sequential access
    /// reader = Reader(
    ///     "https://example.com/data.hxz",
    ///     prefetch_count=16
    /// )
    /// ```
    #[new]
    #[pyo3(signature = (path, s3_region=None, endpoint_url=None, allow_restricted=false, prefetch_count=0, cache_capacity_bytes=None))]
    fn new(
        py: Python<'_>,
        path: String,
        s3_region: Option<String>,
        endpoint_url: Option<String>,
        allow_restricted: bool,
        prefetch_count: u32,
        cache_capacity_bytes: Option<usize>,
    ) -> PyResult<Self> {
        let config = OpenConfig {
            path: path.clone(),
            s3_region,
            endpoint_url,
            allow_restricted,
            prefetch_count,
            cache_capacity_bytes,
        };

        let inner = py.allow_threads(move || -> PyResult<Arc<File>> {
            engine::open_snapshot(config).map_err(|e| PyIOError::new_err(e.to_string()))
        })?;

        Ok(Reader {
            inner,
            path,
            cursor: Mutex::new(0),
        })
    }

    /// Get the total uncompressed size of the snapshot in bytes.
    ///
    /// Returns the logical size of the primary stream. This is the size of the original
    /// uncompressed data, not the size of the compressed snapshot file on disk.
    ///
    /// # Returns
    ///
    /// Total uncompressed size in bytes (unsigned 64-bit integer).
    ///
    /// # Performance
    ///
    /// This is an O(1) operation that reads from the cached header.
    ///
    /// # Python Example
    ///
    /// ```python
    /// reader = Reader("data.hxz")
    /// size = reader.size()
    /// print(f"Uncompressed size: {size / (1024**3):.2f} GB")
    /// ```
    fn size(&self) -> u64 {
        self.inner.size(SnapshotStream::Primary)
    }

    /// Read bytes from the snapshot.
    ///
    /// If `offset` is None, reads from the current cursor position and advances it.
    /// If `offset` is provided, reads from that absolute position without moving the cursor.
    ///
    /// # Python Example
    ///
    /// ```python
    /// from hexz import Reader
    ///
    /// # Open a snapshot
    /// reader = Reader("snapshot.hxz")
    ///
    /// # Read 4096 bytes from the beginning
    /// data = reader.read(size=4096, offset=0)
    /// print(f"Read {len(data)} bytes")
    ///
    /// # Sequential reads using cursor
    /// chunk1 = reader.read(size=1024)  # reads from position 0
    /// chunk2 = reader.read(size=1024)  # reads from position 1024
    ///
    /// # Random access without moving cursor
    /// data_at_offset = reader.read(size=512, offset=8192)
    /// ```
    #[pyo3(signature = (size=None, offset=None))]
    fn read<'py>(
        &self,
        py: Python<'py>,
        size: Option<usize>,
        offset: Option<u64>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let inner = self.inner.clone();
        let total_size = self.inner.size(SnapshotStream::Primary);

        // Compute (start, len, update_cursor) from either the explicit offset
        // or the internal cursor position.
        let (start, len, update_cursor) = match offset {
            Some(at) => {
                if at >= total_size {
                    return Ok(PyBytes::new(py, &[]));
                }
                let len = match size {
                    Some(s) => std::cmp::min(s as u64, total_size - at) as usize,
                    None => (total_size - at) as usize,
                };
                (at, len, false)
            }
            None => {
                let cursor_val = *self
                    .cursor
                    .lock()
                    .map_err(|_| PyRuntimeError::new_err("Cursor lock poisoned"))?;
                if cursor_val >= total_size {
                    return Ok(PyBytes::new(py, &[]));
                }
                let len = match size {
                    Some(s) => std::cmp::min(s as u64, total_size - cursor_val) as usize,
                    None => (total_size - cursor_val) as usize,
                };
                (cursor_val, len, true)
            }
        };

        // Zero-copy: decompress directly into the PyBytes internal buffer.
        // Release the GIL during I/O so HTTP-backend readers don't deadlock
        // when the server runs in a Python thread (same usize pointer pattern
        // used by read_at_into below).
        let bytes = PyBytes::new_with(py, len, |buf| {
            let ptr_addr = buf.as_mut_ptr() as usize;
            let buf_len = buf.len();
            py.allow_threads(move || {
                let slice = unsafe { std::slice::from_raw_parts_mut(ptr_addr as *mut u8, buf_len) };
                inner
                    .read_at_into_uninit_bytes(SnapshotStream::Primary, start, slice)
                    .map_err(|e| PyIOError::new_err(e.to_string()))
            })
        })?;

        if update_cursor {
            let mut cursor = self
                .cursor
                .lock()
                .map_err(|_| PyRuntimeError::new_err("Cursor lock poisoned"))?;
            *cursor = start + len as u64;
        }

        Ok(bytes)
    }

    /// Read at `offset` into a writable buffer (e.g. bytearray). Returns number of bytes read.
    ///
    /// This method provides zero-copy reads into pre-allocated buffers, which is more
    /// efficient than `read()` when working with NumPy arrays or other buffer objects.
    ///
    /// Python buffers are always initialized (e.g. bytearray is zeroed); we use the
    /// write-only (uninit) path because we overwrite the range entirely.
    ///
    /// # Python Example
    ///
    /// ```python
    /// from hexz import Reader
    /// import numpy as np
    ///
    /// reader = Reader("snapshot.hxz")
    ///
    /// # Read into a NumPy array (zero-copy)
    /// buffer = np.zeros(4096, dtype=np.uint8)
    /// bytes_read = reader.read(buffer=buffer, offset=0)
    /// print(f"Read {bytes_read} bytes into NumPy array")
    ///
    /// # Read into a bytearray
    /// ba = bytearray(1024)
    /// bytes_read = reader.read(buffer=ba, offset=8192)
    /// ```
    #[pyo3(name = "_read_at_into")]
    fn read_at_into(
        &self,
        py: Python<'_>,
        offset: u64,
        buffer: Bound<'_, PyAny>,
    ) -> PyResult<usize> {
        let buf_info = tensor::numpy::acquire_writable_buffer(&buffer)?;
        let stream_size = self.inner.size(SnapshotStream::Primary);
        if offset >= stream_size {
            return Ok(0);
        }
        let read_len = std::cmp::min(buf_info.len, (stream_size - offset) as usize);
        let inner = self.inner.clone();
        let ptr_addr = buf_info.ptr as usize;
        let result = py.allow_threads(move || {
            let slice = unsafe { std::slice::from_raw_parts_mut(ptr_addr as *mut u8, read_len) };
            inner
                .read_at_into_uninit_bytes(SnapshotStream::Primary, offset, slice)
                .map(|_| read_len)
                .map_err(|e| PyIOError::new_err(e.to_string()))
        })?;
        Ok(result)
    }

    /// Read from current cursor position into a writable buffer (cursor-based).
    ///
    /// This method provides zero-copy reads into pre-allocated buffers starting from
    /// the current cursor position. After reading, the cursor advances by the number
    /// of bytes read. This is analogous to Python's standard `io.BufferedReader.readinto()`.
    ///
    /// # Arguments
    ///
    /// - `buffer`: Writable buffer object (bytearray, NumPy array, memoryview, etc.)
    ///
    /// # Returns
    ///
    /// Number of bytes read into the buffer. May be less than buffer size if EOF is reached.
    /// Returns 0 if cursor is already at EOF.
    ///
    /// # Performance
    ///
    /// This method avoids allocating intermediate byte arrays by writing directly into
    /// the provided buffer. Prefer this over `read()` when working with pre-allocated
    /// buffers.
    ///
    /// # Python Example
    ///
    /// ```python
    /// import numpy as np
    ///
    /// reader = Reader("data.hxz")
    ///
    /// # Read into NumPy array (zero-copy)
    /// buffer = np.zeros(4096, dtype=np.uint8)
    /// bytes_read = reader.readinto(buffer)
    /// print(f"Read {bytes_read} bytes, cursor now at {reader.tell()}")
    ///
    /// # Read next chunk
    /// bytes_read = reader.readinto(buffer)
    /// print(f"Read {bytes_read} bytes, cursor now at {reader.tell()}")
    ///
    /// # Read into bytearray
    /// ba = bytearray(1024)
    /// bytes_read = reader.readinto(ba)
    /// ```
    fn readinto(&self, py: Python<'_>, buffer: Bound<'_, PyAny>) -> PyResult<usize> {
        let total_size = self.inner.size(SnapshotStream::Primary);

        // Capture cursor value and release lock before I/O (matches read() pattern).
        let start = {
            let cursor = self
                .cursor
                .lock()
                .map_err(|_| PyRuntimeError::new_err("Cursor lock poisoned"))?;
            *cursor
        };

        if start >= total_size {
            return Ok(0);
        }

        let buf_info = tensor::numpy::acquire_writable_buffer(&buffer)?;
        let read_len = std::cmp::min(buf_info.len, (total_size - start) as usize);

        let inner = self.inner.clone();

        // Cast pointer to usize to allow sending to allow_threads closure (usize is Send)
        let ptr_addr = buf_info.ptr as usize;

        // Release GIL for reading (lock is NOT held during I/O)
        let result = py.allow_threads(move || {
            // SAFETY: buf_info.ptr is valid for buf_info.len bytes and writable.
            // We clamped read_len to buf_info.len.
            // The buffer is kept alive by buf_info in the outer scope (which waits for this closure).
            let ptr = ptr_addr as *mut u8;
            let slice = unsafe { std::slice::from_raw_parts_mut(ptr, read_len) };

            inner
                .read_at_into_uninit_bytes(SnapshotStream::Primary, start, slice)
                .map(|_| read_len)
                .map_err(|e| PyIOError::new_err(e.to_string()))
        })?;

        // Re-acquire lock only for the cursor update
        {
            let mut cursor = self
                .cursor
                .lock()
                .map_err(|_| PyRuntimeError::new_err("Cursor lock poisoned"))?;
            *cursor = start + result as u64;
        }

        Ok(result)
    }

    /// Seek to a position in the snapshot.
    ///
    /// Changes the current cursor position for subsequent reads without an explicit offset.
    /// Follows Python's standard `seek()` semantics with `whence` parameter.
    ///
    /// # Arguments
    ///
    /// - `offset`: The offset to seek to (interpretation depends on `whence`)
    /// - `whence`: Optional seek mode (default: 0)
    ///   - 0 (SEEK_SET): Seek from start of file
    ///   - 1 (SEEK_CUR): Seek relative to current position
    ///   - 2 (SEEK_END): Seek from end of file
    ///
    /// # Python Example
    ///
    /// ```python
    /// from hexz import Reader
    /// import os
    ///
    /// reader = Reader("snapshot.hxz")
    ///
    /// # Seek to absolute position
    /// reader.seek(4096)
    /// data = reader.read(512)  # reads from position 4096
    ///
    /// # Seek relative to current position
    /// reader.seek(1024, os.SEEK_CUR)
    ///
    /// # Seek from end
    /// reader.seek(-4096, os.SEEK_END)
    /// trailer = reader.read(4096)  # reads last 4 KiB
    ///
    /// # Get current position
    /// pos = reader.tell()
    /// ```
    #[pyo3(signature = (offset, whence=None))]
    fn seek(&self, offset: i64, whence: Option<i32>) -> PyResult<u64> {
        let mut cursor = self
            .cursor
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Cursor lock poisoned"))?;
        let total_size = self.inner.size(SnapshotStream::Primary);

        let new_pos = match whence.unwrap_or(0) {
            0 => offset,
            1 => *cursor as i64 + offset,
            2 => total_size as i64 + offset,
            _ => return Err(PyValueError::new_err("Invalid whence argument")),
        };

        if new_pos < 0 {
            return Err(PyValueError::new_err("Seek before start of file"));
        }

        *cursor = new_pos as u64;
        Ok(*cursor)
    }

    /// Get the current cursor position.
    ///
    /// Returns the current position in the snapshot where the next cursor-based read
    /// (without explicit `offset` parameter) will start.
    ///
    /// # Returns
    ///
    /// Current cursor position in bytes (unsigned 64-bit integer).
    ///
    /// # Python Example
    ///
    /// ```python
    /// reader = Reader("data.hxz")
    /// reader.read(1024)  # cursor advances to 1024
    /// pos = reader.tell()  # returns 1024
    /// ```
    fn tell(&self) -> PyResult<u64> {
        Ok(*self
            .cursor
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Cursor lock poisoned"))?)
    }

    /// Check if the snapshot is readable.
    ///
    /// Always returns `True` for `Reader` instances.
    ///
    /// # Returns
    ///
    /// `True` (snapshots are always readable).
    fn readable(&self) -> bool {
        true
    }

    /// Check if the snapshot is seekable.
    ///
    /// Always returns `True` for `Reader` instances.
    ///
    /// # Returns
    ///
    /// `True` (snapshots always support seeking).
    fn seekable(&self) -> bool {
        true
    }

    /// Check if the snapshot is writable.
    ///
    /// Always returns `False` for `Reader` instances (read-only).
    ///
    /// # Returns
    ///
    /// `False` (snapshots are read-only).
    fn writable(&self) -> bool {
        false
    }

    /// Flush write buffers (no-op for read-only snapshots).
    ///
    /// Included for compatibility with Python's file protocol. Does nothing for readers.
    fn flush(&self) {}

    /// Get file descriptor (not supported for virtual streams).
    ///
    /// # Raises
    ///
    /// `OSError`: Always raised because `Reader` is a virtual stream without
    /// an associated OS file descriptor.
    fn fileno(&self) -> PyResult<i32> {
        Err(PyOSError::new_err("Reader is a virtual file stream"))
    }

    /// Get snapshot metadata including format information and statistics.
    ///
    /// Returns a dictionary containing header information, compression settings,
    /// size statistics, and other snapshot properties.
    ///
    /// # Python Example
    ///
    /// ```python
    /// from hexz import Reader
    ///
    /// reader = Reader("snapshot.hxz")
    /// meta = reader.metadata()
    ///
    /// print(f"Format version: {meta['version']}")
    /// print(f"Compression: {meta['compression']}")
    /// print(f"Block size: {meta['block_size']}")
    /// print(f"Primary size: {meta['primary_size']} bytes")
    /// print(f"Encrypted: {meta.get('encrypted', False)}")
    /// ```
    fn metadata(&self, py: Python<'_>) -> PyResult<PyObject> {
        use hexz_core::format::version::{
            CURRENT_VERSION, MAX_SUPPORTED_VERSION, MIN_SUPPORTED_VERSION, check_version,
            compatibility_message,
        };
        use pyo3::types::PyDict;

        let header = &self.inner.header;
        let dict = PyDict::new(py);

        // Version and compatibility — derived from the cached header.
        let version = header.version;
        let compatibility = check_version(version);
        let is_compatible = compatibility.is_compatible();
        let compatibility_status = match compatibility {
            hexz_core::format::version::VersionCompatibility::Full => "full",
            hexz_core::format::version::VersionCompatibility::Degraded => "degraded",
            hexz_core::format::version::VersionCompatibility::Incompatible => "incompatible",
        };

        dict.set_item("version", version)?;
        dict.set_item("current_version", CURRENT_VERSION)?;
        dict.set_item("min_supported_version", MIN_SUPPORTED_VERSION)?;
        dict.set_item("max_supported_version", MAX_SUPPORTED_VERSION)?;
        dict.set_item("is_compatible", is_compatible)?;
        dict.set_item("compatibility_status", compatibility_status)?;
        dict.set_item("compatibility_message", compatibility_message(version))?;
        dict.set_item("block_size", header.block_size)?;
        dict.set_item("compression", format!("{:?}", header.compression))?;
        dict.set_item("encrypted", header.encryption.is_some())?;
        dict.set_item("parent_paths", &header.parent_paths)?;

        // Stream sizes from the cached master index.
        let primary_size = self.inner.size(SnapshotStream::Primary);
        let secondary_size = self.inner.size(SnapshotStream::Secondary);
        dict.set_item("primary_size", primary_size)?;
        dict.set_item("secondary_size", secondary_size)?;

        // File size — cheap stat for local paths, 0 for remote.
        let file_size = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        dict.set_item("file_size", file_size)?;

        let total_uncompressed = primary_size + secondary_size;
        let ratio = if file_size > 0 {
            total_uncompressed as f64 / file_size as f64
        } else {
            0.0
        };
        dict.set_item("ratio", ratio)?;

        // User metadata — read only the small metadata region if present.
        if let (Some(offset), Some(length)) = (header.metadata_offset, header.metadata_length) {
            if length > 0 {
                if let Ok(mut f) = std::fs::File::open(&self.path) {
                    use std::io::{Read, Seek, SeekFrom};
                    if f.seek(SeekFrom::Start(offset)).is_ok() {
                        let mut meta_bytes = vec![0u8; length as usize];
                        if f.read_exact(&mut meta_bytes).is_ok() {
                            if let Ok(json) = py.import("json") {
                                let bytes_obj = pyo3::types::PyBytes::new(py, &meta_bytes);
                                if let Ok(user_meta) = json.call_method1("loads", (bytes_obj,)) {
                                    if let Ok(user_dict) = user_meta.downcast::<PyDict>() {
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
                }
            }
        }

        Ok(dict.into())
    }

    /// Close the snapshot reader (no-op).
    ///
    /// Included for compatibility with Python's file protocol. Reader uses
    /// reference counting for resource management, so explicit closing is not required.
    fn close(&self) {}

    /// Enter context manager (returns self).
    ///
    /// Enables usage with Python's `with` statement:
    ///
    /// ```python
    /// with Reader("data.hxz") as reader:
    ///     data = reader.read(1024)
    /// ```
    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    /// Exit context manager (cleanup on scope exit).
    ///
    /// Called automatically when exiting a `with` block. Currently a no-op since
    /// resource cleanup is handled by reference counting.
    fn __exit__(&self, _exc_type: PyObject, _exc_value: PyObject, _traceback: PyObject) {}

    /// Get constructor arguments for pickling.
    ///
    /// Returns the path argument used to construct this reader. Used by Python's
    /// pickle protocol to reconstruct the reader instance.
    ///
    /// # Returns
    ///
    /// Tuple containing the snapshot path.
    fn __getnewargs__(&self) -> PyResult<(String,)> {
        Ok((self.path.clone(),))
    }

    /// Get state for pickling.
    ///
    /// Returns the current cursor position. Used by Python's pickle protocol
    /// to serialize the reader's state.
    ///
    /// # Returns
    ///
    /// Current cursor position as unsigned 64-bit integer.
    fn __getstate__(&self) -> PyResult<u64> {
        Ok(*self
            .cursor
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Cursor lock poisoned"))?)
    }

    /// Restore state from pickling.
    ///
    /// Restores the cursor position after unpickling. Used by Python's pickle
    /// protocol to deserialize the reader's state.
    ///
    /// # Arguments
    ///
    /// - `state`: Cursor position to restore
    fn __setstate__(&self, state: u64) -> PyResult<()> {
        *self
            .cursor
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Cursor lock poisoned"))? = state;
        Ok(())
    }
}
