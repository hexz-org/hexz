//! Python dataset classes wrapping the Rust engine.
//!
//! Provides `StrataReader` (synchronous) and `AsyncStrataReader` (asyncio)
//! Python classes that delegate to the pure-Rust engine layer.

use pyo3::exceptions::{PyIOError, PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::sync::{Arc, Mutex};
use strata_core::StrataFile;
use strata_core::api::stratafile::SnapshotStream;

use crate::engine::{self, OpenConfig};
use crate::tensor;

#[pyclass(module = "strata.strata_loader")]
pub struct StrataReader {
    pub(crate) inner: Arc<StrataFile>,
    path: String,
    cursor: Mutex<u64>,
}

#[pymethods]
impl StrataReader {
    #[new]
    #[pyo3(signature = (path, s3_region=None, endpoint_url=None, allow_restricted=false))]
    fn new(
        py: Python<'_>,
        path: String,
        s3_region: Option<String>,
        endpoint_url: Option<String>,
        allow_restricted: bool,
    ) -> PyResult<Self> {
        let config = OpenConfig {
            path: path.clone(),
            s3_region,
            endpoint_url,
            allow_restricted,
        };

        let inner = py.allow_threads(move || -> PyResult<Arc<StrataFile>> {
            engine::open_snapshot(config).map_err(|e| PyIOError::new_err(e.to_string()))
        })?;

        Ok(StrataReader {
            inner,
            path,
            cursor: Mutex::new(0),
        })
    }

    fn size(&self) -> u64 {
        self.inner.size(SnapshotStream::Disk)
    }

    /// Read bytes from the snapshot.
    ///
    /// If `offset` is None, reads from the current cursor position and advances it.
    /// If `offset` is provided, reads from that absolute position without moving the cursor.
    ///
    /// # Python Example
    ///
    /// ```python
    /// from strata import StrataReader
    ///
    /// # Open a snapshot
    /// reader = StrataReader("snapshot.st")
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
        let total_size = self.inner.size(SnapshotStream::Disk);

        let start = match offset {
            None => {
                let mut cursor = self.cursor.lock().unwrap();
                if *cursor >= total_size {
                    return Ok(PyBytes::new(py, &[]));
                }
                let start = *cursor;
                let len = match size {
                    Some(s) => std::cmp::min(s as u64, total_size - *cursor) as usize,
                    None => (total_size - *cursor) as usize,
                };
                let data = py
                    .allow_threads({
                        let inner = inner.clone();
                        move || inner.read_at(SnapshotStream::Disk, start, len)
                    })
                    .map_err(|e| PyIOError::new_err(e.to_string()))?;
                *cursor += data.len() as u64;
                return Ok(PyBytes::new(py, &data));
            }
            Some(at) => at,
        };

        if start >= total_size {
            return Ok(PyBytes::new(py, &[]));
        }
        let len = match size {
            Some(s) => std::cmp::min(s as u64, total_size - start) as usize,
            None => (total_size - start) as usize,
        };
        let data = py
            .allow_threads(move || inner.read_at(SnapshotStream::Disk, start, len))
            .map_err(|e| PyIOError::new_err(e.to_string()))?;
        Ok(PyBytes::new(py, &data))
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
    /// from strata import StrataReader
    /// import numpy as np
    ///
    /// reader = StrataReader("snapshot.st")
    ///
    /// # Read into a NumPy array (zero-copy)
    /// buffer = np.zeros(4096, dtype=np.uint8)
    /// bytes_read = reader.read_at_into(offset=0, buffer=buffer)
    /// print(f"Read {bytes_read} bytes into NumPy array")
    ///
    /// # Read into a bytearray
    /// ba = bytearray(1024)
    /// bytes_read = reader.read_at_into(offset=8192, buffer=ba)
    /// ```
    fn read_at_into(
        &self,
        py: Python<'_>,
        offset: u64,
        buffer: Bound<'_, PyAny>,
    ) -> PyResult<usize> {
        let buf_info = tensor::numpy::acquire_writable_buffer(&buffer)?;
        let stream_size = self.inner.size(SnapshotStream::Disk);
        if offset >= stream_size {
            return Ok(0);
        }
        let read_len = std::cmp::min(buf_info.len, (stream_size - offset) as usize);
        let inner = self.inner.clone();
        let ptr_addr = buf_info.ptr as usize;
        let result = py.allow_threads(move || {
            let slice = unsafe { std::slice::from_raw_parts_mut(ptr_addr as *mut u8, read_len) };
            inner
                .read_at_into_uninit_bytes(SnapshotStream::Disk, offset, slice)
                .map(|_| read_len)
                .map_err(|e| PyIOError::new_err(e.to_string()))
        })?;
        Ok(result)
    }

    fn readinto(&self, py: Python<'_>, buffer: Bound<'_, PyAny>) -> PyResult<usize> {
        let mut cursor = self.cursor.lock().unwrap();
        let total_size = self.inner.size(SnapshotStream::Disk);

        if *cursor >= total_size {
            return Ok(0);
        }

        let buf_info = tensor::numpy::acquire_writable_buffer(&buffer)?;
        let read_len = std::cmp::min(buf_info.len, (total_size - *cursor) as usize);

        let inner = self.inner.clone();
        let start = *cursor;

        // Cast pointer to usize to allow sending to allow_threads closure (usize is Send)
        let ptr_addr = buf_info.ptr as usize;

        // Release GIL for reading
        let result = py.allow_threads(move || {
            // SAFETY: buf_info.ptr is valid for buf_info.len bytes and writable.
            // We clamped read_len to buf_info.len.
            // The buffer is kept alive by buf_info in the outer scope (which waits for this closure).
            let ptr = ptr_addr as *mut u8;
            let slice = unsafe { std::slice::from_raw_parts_mut(ptr, read_len) };

            inner
                .read_at_into_uninit_bytes(SnapshotStream::Disk, start, slice)
                .map(|_| read_len)
                .map_err(|e| PyIOError::new_err(e.to_string()))
        })?;

        *cursor += result as u64;
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
    /// from strata import StrataReader
    /// import os
    ///
    /// reader = StrataReader("snapshot.st")
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
        let mut cursor = self.cursor.lock().unwrap();
        let total_size = self.inner.size(SnapshotStream::Disk);

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

    fn tell(&self) -> u64 {
        *self.cursor.lock().unwrap()
    }

    fn readable(&self) -> bool {
        true
    }
    fn seekable(&self) -> bool {
        true
    }
    fn writable(&self) -> bool {
        false
    }
    fn flush(&self) {}
    fn fileno(&self) -> PyResult<i32> {
        Err(PyOSError::new_err("StrataReader is a virtual file stream"))
    }

    /// Get snapshot metadata including format information and statistics.
    ///
    /// Returns a dictionary containing header information, compression settings,
    /// size statistics, and other snapshot properties.
    ///
    /// # Python Example
    ///
    /// ```python
    /// from strata import StrataReader
    ///
    /// reader = StrataReader("snapshot.st")
    /// meta = reader.metadata()
    ///
    /// print(f"Format version: {meta['version']}")
    /// print(f"Compression: {meta['compression']}")
    /// print(f"Block size: {meta['block_size']}")
    /// print(f"Disk size: {meta['disk_size']} bytes")
    /// print(f"Encrypted: {meta.get('encrypted', False)}")
    /// ```
    fn metadata(&self, py: Python<'_>) -> PyResult<PyObject> {
        // Delegate to the inspect function to get metadata
        use pyo3::types::PyDict;

        // Call the inspect function from ops module
        let meta = super::ops::inspect(py, self.path.clone())?;

        // Convert HashMap to PyDict
        let dict = PyDict::new(py);
        for (key, value) in meta {
            dict.set_item(key, value)?;
        }

        Ok(dict.into())
    }

    fn close(&self) {}

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }
    fn __exit__(&self, _exc_type: PyObject, _exc_value: PyObject, _traceback: PyObject) {}

    fn __getnewargs__(&self) -> PyResult<(String,)> {
        Ok((self.path.clone(),))
    }
    fn __getstate__(&self) -> PyResult<u64> {
        Ok(*self.cursor.lock().unwrap())
    }
    fn __setstate__(&self, state: u64) -> PyResult<()> {
        *self.cursor.lock().unwrap() = state;
        Ok(())
    }
}
