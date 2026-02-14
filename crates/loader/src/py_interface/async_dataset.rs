//! Asynchronous Python snapshot reader with native asyncio integration.
//!
//! This module provides `AsyncReader`, an async/await-compatible Python class for
//! reading Hexz snapshot files within asyncio applications. All I/O operations return
//! Python coroutines that integrate seamlessly with the asyncio event loop.
//!
//! # Overview
//!
//! `AsyncReader` wraps the synchronous `hexz_core::File` engine with async
//! Python bindings using `pyo3-async-runtimes`. All blocking operations (decompression,
//! I/O) are executed on Tokio's blocking thread pool via `tokio::task::spawn_blocking`,
//! ensuring they never block the asyncio event loop.
//!
//! # Key Features
//!
//! - **Native Asyncio Integration**: All methods return Python coroutines (`async def`)
//! - **Non-Blocking I/O**: Operations run on Tokio's blocking pool, never blocking event loop
//! - **Concurrent Access**: Multiple coroutines can safely access the same reader concurrently
//! - **Context Manager Support**: Implements `async with` via `__aenter__`/`__aexit__`
//! - **Cursor and Absolute Modes**: Same dual-mode access as synchronous `Reader`
//!
//! # Architecture
//!
//! The async implementation follows this execution flow:
//!
//! 1. Python coroutine is invoked (e.g., `await reader.read(1024)`)
//! 2. PyO3 converts the call to a Rust future via `pyo3_async_runtimes::tokio::future_into_py`
//! 3. The future spawns a blocking task via `tokio::task::spawn_blocking`
//! 4. The blocking task performs I/O and decompression on a dedicated thread pool
//! 5. Results are converted back to Python objects and returned to the awaiting coroutine
//!
//! This ensures asyncio applications maintain responsiveness even during heavy I/O operations.
//!
//! # Tokio Runtime Requirements
//!
//! `AsyncReader` requires a Tokio runtime to be active. When using `asyncio`, the
//! runtime is automatically managed by `pyo3-async-runtimes`. Ensure your Python environment
//! has a running event loop:
//!
//! ```python
//! import asyncio
//!
//! async def main():
//!     reader = await AsyncReader.create("data.hxz")
//!     data = await reader.read(1024)
//!     return data
//!
//! # Run with asyncio
//! result = asyncio.run(main())
//! ```
//!
//! # Concurrent Access Patterns
//!
//! Multiple coroutines can safely access the same `AsyncReader` instance concurrently.
//! However, cursor-based reads serialize internally via the `Mutex`-protected cursor:
//!
//! ```python
//! async def read_chunk(reader, size):
//!     return await reader.read(size)
//!
//! # Concurrent cursor-based reads (serialize on cursor lock)
//! reader = await AsyncReader.create("data.hxz")
//! results = await asyncio.gather(
//!     read_chunk(reader, 1024),
//!     read_chunk(reader, 1024),
//!     read_chunk(reader, 1024),
//! )
//! # Results will be sequential chunks due to cursor advancement
//! ```
//!
//! For true parallel random access, use explicit offsets:
//!
//! ```python
//! # Concurrent random access reads (fully parallel)
//! reader = await AsyncReader.create("data.hxz")
//! results = await asyncio.gather(
//!     reader.read(1024, offset=0),
//!     reader.read(1024, offset=4096),
//!     reader.read(1024, offset=8192),
//! )
//! # All reads execute concurrently on separate blocking threads
//! ```
//!
//! # Performance Considerations
//!
//! - **Blocking Pool Size**: Tokio's default blocking pool has limited threads. For high
//!   concurrency, consider increasing the pool size via `TOKIO_BLOCKING_THREADS` environment
//!   variable.
//!
//! - **Overhead**: Each async operation incurs small overhead from thread pool dispatch.
//!   For very small reads (<1KB), synchronous `Reader` may be faster.
//!
//! - **Caching**: Configure `cache_capacity_bytes` and `prefetch_count` just like synchronous
//!   readers. Cache is shared across all coroutines accessing the same reader.
//!
//! # Integration Examples
//!
//! ## ASGI Web Application
//!
//! ```python
//! from starta import AsyncReader
//! from fastapi import FastAPI
//! from fastapi.responses import Response
//!
//! app = FastAPI()
//! reader = None
//!
//! @app.on_event("startup")
//! async def startup():
//!     global reader
//!     reader = await AsyncReader.create("data.hxz")
//!
//! @app.get("/data/{offset}/{size}")
//! async def get_data(offset: int, size: int):
//!     data = await reader.read(size, offset=offset)
//!     return Response(content=bytes(data))
//! ```
//!
//! ## Async Batch Processing
//!
//! ```python
//! import asyncio
//! from hexz import AsyncReader
//!
//! async def process_batch(reader, batch_offsets, size):
//!     tasks = [reader.read(size, offset=off) for off in batch_offsets]
//!     chunks = await asyncio.gather(*tasks)
//!     return [process(chunk) for chunk in chunks]
//!
//! async def main():
//!     reader = await AsyncReader.create("data.hxz")
//!     batch_size = 1024
//!     offsets = range(0, 1000000, batch_size)
//!
//!     # Process in batches of 100 concurrent reads
//!     for i in range(0, len(offsets), 100):
//!         batch = offsets[i:i+100]
//!         results = await process_batch(reader, batch, batch_size)
//!         # Process results...
//!
//! asyncio.run(main())
//! ```
//!
//! # Thread Safety
//!
//! `AsyncReader` is async-safe: multiple coroutines can safely share the same instance.
//! The cursor is protected by a `Mutex`, and all I/O operations run on Tokio's blocking pool,
//! preventing event loop blocking.

use hexz_core::File;
use hexz_core::api::file::SnapshotStream;
use pyo3::exceptions::{PyIOError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::sync::{Arc, Mutex};

use crate::engine::{self, OpenConfig};

/// Asynchronous Python reader for Hexz snapshot files.
///
/// `AsyncReader` provides an async/await-compatible interface for reading compressed
/// snapshot files from within asyncio applications. All I/O operations return coroutines
/// that execute on Tokio's blocking thread pool, ensuring the event loop remains responsive.
///
/// # Factory Method
///
/// Unlike synchronous `Reader` which uses `__init__`, `AsyncReader` uses a
/// static factory method `create()` that returns a coroutine:
///
/// ```python
/// reader = await AsyncReader.create("data.hxz")
/// ```
///
/// This is required because `__init__` cannot be async in Python.
///
/// # Parameters (via `create()`)
///
/// - `path` (str): Path to snapshot file (local, HTTP, or S3)
/// - `s3_region` (str, optional): AWS region for S3 URIs
/// - `endpoint_url` (str, optional): Custom S3 endpoint URL
/// - `allow_restricted` (bool, default=False): Allow connections to private IPs
/// - `prefetch_count` (int, default=0): Number of blocks to prefetch in background
/// - `cache_capacity_bytes` (int, optional): Block cache size in bytes
///
/// # Methods
///
/// All methods return coroutines that must be awaited:
///
/// - `await reader.read(size, offset=None)`: Read bytes
/// - `await reader.seek(offset, whence=0)`: Change cursor position
/// - `reader.tell()`: Get cursor position (synchronous)
/// - `reader.size()`: Get total uncompressed size (synchronous)
///
/// # Python Example
///
/// ```python
/// import asyncio
/// from hexz import AsyncReader
///
/// async def main():
///     # Create reader (async operation)
///     reader = await AsyncReader.create("data.hxz", prefetch_count=16)
///
///     # Get size (synchronous - no await needed)
///     total = reader.size()
///     print(f"Total size: {total} bytes")
///
///     # Sequential reads (cursor-based)
///     chunk1 = await reader.read(4096)
///     chunk2 = await reader.read(4096)
///
///     # Random access (absolute offset)
///     data = await reader.read(1024, offset=8192)
///
///     # Seek operations
///     await reader.seek(0)  # rewind
///
///     # Async context manager
///     async with await AsyncReader.create("data.hxz") as reader:
///         data = await reader.read(1024)
///
/// asyncio.run(main())
/// ```
#[pyclass(module = "hexz.hexz_loader")]
pub struct AsyncReader {
    inner: Arc<File>,
    cursor: Arc<Mutex<u64>>,
}

#[pymethods]
impl AsyncReader {
    /// Create a new AsyncReader instance (async factory method).
    ///
    /// Opens a snapshot file asynchronously for reading with optional configuration.
    /// This is a static method that returns a coroutine yielding an `AsyncReader`.
    ///
    /// # Arguments
    ///
    /// - `path`: Path to snapshot file (local, HTTP, or S3)
    /// - `s3_region`: AWS region for S3 URLs (e.g., "us-west-2")
    /// - `endpoint_url`: Custom S3 endpoint URL for MinIO, Ceph, etc.
    /// - `allow_restricted`: Allow connections to private/internal IP addresses
    /// - `prefetch_count`: Number of blocks to prefetch (0 = disabled)
    /// - `cache_capacity_bytes`: Block cache size in bytes (None = default ~4MB)
    ///
    /// # Returns
    ///
    /// Coroutine that yields a new `AsyncReader` instance positioned at offset 0.
    ///
    /// # Raises
    ///
    /// - `IOError`: File not found, permission denied, or network error
    /// - `FormatError`: Invalid snapshot format or corrupted header
    /// - `VersionError`: Unsupported format version
    /// - `RuntimeError`: Tokio runtime error (rare)
    ///
    /// # Python Example
    ///
    /// ```python
    /// # Local file with defaults
    /// reader = await AsyncReader.create("data.hxz")
    ///
    /// # S3 with region and large cache
    /// reader = await AsyncReader.create(
    ///     "s3://bucket/data.hxz",
    ///     s3_region="us-west-2",
    ///     cache_capacity_bytes=1024*1024*1024  # 1 GB
    /// )
    ///
    /// # HTTP with prefetching
    /// reader = await AsyncReader.create(
    ///     "https://example.com/data.hxz",
    ///     prefetch_count=16
    /// )
    /// ```
    #[staticmethod]
    #[pyo3(signature = (path, s3_region=None, endpoint_url=None, allow_restricted=false, prefetch_count=0, cache_capacity_bytes=None))]
    fn create(
        py: Python<'_>,
        path: String,
        s3_region: Option<String>,
        endpoint_url: Option<String>,
        allow_restricted: bool,
        prefetch_count: u32,
        cache_capacity_bytes: Option<usize>,
    ) -> PyResult<Bound<'_, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let core = tokio::task::spawn_blocking(move || -> PyResult<Arc<File>> {
                let config = OpenConfig {
                    path,
                    s3_region,
                    endpoint_url,
                    allow_restricted,
                    prefetch_count,
                    cache_capacity_bytes,
                };
                engine::open_snapshot(config).map_err(|e| PyIOError::new_err(e.to_string()))
            })
            .await
            .map_err(|e: tokio::task::JoinError| PyRuntimeError::new_err(e.to_string()))??;

            let reader = AsyncReader {
                inner: core,
                cursor: Arc::new(Mutex::new(0)),
            };

            Ok(reader)
        })
    }

    /// Get the total uncompressed size of the snapshot in bytes (synchronous).
    ///
    /// Returns the logical size of the disk stream. This is the size of the original
    /// uncompressed data, not the size of the compressed snapshot file on disk.
    ///
    /// This method is synchronous (no `await` needed) because it reads from cached header data.
    ///
    /// # Returns
    ///
    /// Total uncompressed size in bytes (unsigned 64-bit integer).
    ///
    /// # Python Example
    ///
    /// ```python
    /// reader = await AsyncReader.create("data.hxz")
    /// size = reader.size()  # synchronous, no await
    /// print(f"Uncompressed size: {size / (1024**3):.2f} GB")
    /// ```
    fn size(&self) -> u64 {
        self.inner.size(SnapshotStream::Disk)
    }

    /// Read bytes asynchronously from the snapshot.
    ///
    /// If `offset` is `None`, reads from the current cursor position and advances the cursor
    /// by the number of bytes read. If `offset` is provided, reads from that absolute position
    /// without modifying the cursor.
    ///
    /// This method returns a coroutine that executes the read operation on Tokio's blocking
    /// thread pool, preventing event loop blocking.
    ///
    /// # Arguments
    ///
    /// - `size` (int, optional): Number of bytes to read. If `None`, reads until EOF.
    /// - `offset` (int, optional): Absolute offset to read from. If `None`, uses cursor.
    ///
    /// # Returns
    ///
    /// Coroutine yielding `bytes` object containing the read data. May be shorter than
    /// requested size if EOF is reached.
    ///
    /// # Raises
    ///
    /// - `IOError`: I/O error during read operation
    /// - `RuntimeError`: Tokio task join error (rare)
    ///
    /// # Python Example
    ///
    /// ```python
    /// reader = await AsyncReader.create("data.hxz")
    ///
    /// # Sequential reads (cursor-based)
    /// chunk1 = await reader.read(4096)
    /// chunk2 = await reader.read(4096)
    ///
    /// # Random access without moving cursor
    /// data = await reader.read(1024, offset=8192)
    ///
    /// # Read until EOF
    /// all_data = await reader.read()
    ///
    /// # Concurrent random access reads
    /// import asyncio
    /// results = await asyncio.gather(
    ///     reader.read(1024, offset=0),
    ///     reader.read(1024, offset=4096),
    ///     reader.read(1024, offset=8192),
    /// )
    /// ```
    #[pyo3(signature = (size=None, offset=None))]
    fn read<'p>(
        &self,
        py: Python<'p>,
        size: Option<usize>,
        offset: Option<u64>,
    ) -> PyResult<Bound<'p, PyAny>> {
        let inner = self.inner.clone();
        let cursor = self.cursor.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let data = tokio::task::spawn_blocking(move || -> PyResult<Vec<u8>> {
                let total_size = inner.size(SnapshotStream::Disk);
                match offset {
                    None => {
                        let mut pos = cursor
                            .lock()
                            .map_err(|_| PyRuntimeError::new_err("Cursor lock poisoned"))?;
                        if *pos >= total_size {
                            return Ok(Vec::new());
                        }
                        let start = *pos;
                        let len = match size {
                            Some(s) => std::cmp::min(s as u64, total_size - *pos) as usize,
                            None => (total_size - *pos) as usize,
                        };
                        let bytes = inner
                            .read_at(SnapshotStream::Disk, start, len)
                            .map_err(|e| PyIOError::new_err(e.to_string()))?;
                        *pos += bytes.len() as u64;
                        Ok(bytes)
                    }
                    Some(at) => {
                        if at >= total_size {
                            return Ok(Vec::new());
                        }
                        let len = match size {
                            Some(s) => std::cmp::min(s as u64, total_size - at) as usize,
                            None => (total_size - at) as usize,
                        };
                        inner
                            .read_at(SnapshotStream::Disk, at, len)
                            .map_err(|e| PyIOError::new_err(e.to_string()))
                    }
                }
            })
            .await
            .map_err(|e: tokio::task::JoinError| PyRuntimeError::new_err(e.to_string()))??;

            Python::with_gil(|py| {
                let bytes = PyBytes::new(py, &data);
                Ok(bytes.unbind().into_any())
            })
        })
    }

    /// Seek to a position in the snapshot asynchronously.
    ///
    /// Changes the current cursor position for subsequent cursor-based reads. Follows
    /// Python's standard `seek()` semantics with `whence` parameter.
    ///
    /// Returns a coroutine that executes the seek operation on Tokio's blocking thread pool.
    ///
    /// # Arguments
    ///
    /// - `offset` (int): The offset to seek to (interpretation depends on `whence`)
    /// - `whence` (int, optional): Seek mode (default: 0)
    ///   - 0 (SEEK_SET): Seek from start of file
    ///   - 1 (SEEK_CUR): Seek relative to current position
    ///   - 2 (SEEK_END): Seek from end of file
    ///
    /// # Returns
    ///
    /// Coroutine yielding the new cursor position as an unsigned 64-bit integer.
    ///
    /// # Raises
    ///
    /// - `ValueError`: Invalid whence argument or seeking before start of file
    /// - `RuntimeError`: Tokio task join error (rare)
    ///
    /// # Python Example
    ///
    /// ```python
    /// import os
    ///
    /// reader = await AsyncReader.create("data.hxz")
    ///
    /// # Seek to absolute position
    /// await reader.seek(4096)
    /// data = await reader.read(512)
    ///
    /// # Seek relative to current position
    /// await reader.seek(1024, os.SEEK_CUR)
    ///
    /// # Seek from end
    /// await reader.seek(-4096, os.SEEK_END)
    /// trailer = await reader.read(4096)
    ///
    /// # Get new position
    /// pos = await reader.seek(0, os.SEEK_CUR)
    /// ```
    #[pyo3(signature = (offset, whence=None))]
    fn seek<'p>(
        &self,
        py: Python<'p>,
        offset: i64,
        whence: Option<i32>,
    ) -> PyResult<Bound<'p, PyAny>> {
        let cursor = self.cursor.clone();
        let inner = self.inner.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let new_pos = tokio::task::spawn_blocking(move || -> PyResult<u64> {
                let mut pos = cursor
                    .lock()
                    .map_err(|_| PyRuntimeError::new_err("Cursor lock poisoned"))?;
                let total_size = inner.size(SnapshotStream::Disk);

                let new_pos = match whence.unwrap_or(0) {
                    0 => offset,
                    1 => *pos as i64 + offset,
                    2 => total_size as i64 + offset,
                    _ => return Err(PyValueError::new_err("Invalid whence argument")),
                };

                if new_pos < 0 {
                    return Err(PyValueError::new_err("Seek before start of file"));
                }

                *pos = new_pos as u64;
                Ok(*pos)
            })
            .await
            .map_err(|e: tokio::task::JoinError| PyRuntimeError::new_err(e.to_string()))??;

            Ok(new_pos)
        })
    }

    /// Get the current cursor position (synchronous).
    ///
    /// Returns the current position in the snapshot where the next cursor-based read
    /// will start. This method is synchronous (no `await` needed) because it reads
    /// from in-memory cursor state.
    ///
    /// # Returns
    ///
    /// Current cursor position in bytes (unsigned 64-bit integer).
    ///
    /// # Python Example
    ///
    /// ```python
    /// reader = await AsyncReader.create("data.hxz")
    /// await reader.read(1024)
    /// pos = reader.tell()  # synchronous, no await
    /// print(f"Cursor at: {pos}")
    /// ```
    fn tell(&self) -> PyResult<u64> {
        Ok(*self
            .cursor
            .lock()
            .map_err(|_| PyRuntimeError::new_err("Cursor lock poisoned"))?)
    }

    /// Enter async context manager (returns self as coroutine).
    ///
    /// Enables usage with Python's `async with` statement:
    ///
    /// ```python
    /// async with await AsyncReader.create("data.hxz") as reader:
    ///     data = await reader.read(1024)
    /// ```
    ///
    /// # Returns
    ///
    /// Coroutine yielding `self`.
    fn __aenter__<'p>(slf: Py<Self>, py: Python<'p>) -> PyResult<Bound<'p, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(slf) })
    }

    /// Exit async context manager (cleanup on scope exit).
    ///
    /// Called automatically when exiting an `async with` block. Currently a no-op
    /// since resource cleanup is handled by reference counting.
    ///
    /// # Returns
    ///
    /// Coroutine yielding `None`.
    fn __aexit__<'p>(
        &self,
        py: Python<'p>,
        _exc_type: PyObject,
        _exc_value: PyObject,
        _traceback: PyObject,
    ) -> PyResult<Bound<'p, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(()) })
    }
}
