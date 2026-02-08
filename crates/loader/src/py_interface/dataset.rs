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

#[pyclass(module = "strata._strata_core")]
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

        let inner = py.allow_threads(move || -> PyResult<StrataFile> {
            engine::open_snapshot(config).map_err(|e| PyIOError::new_err(e.to_string()))
        })?;

        Ok(StrataReader {
            inner: Arc::new(inner),
            path,
            cursor: Mutex::new(0),
        })
    }

    fn size(&self) -> u64 {
        self.inner.size(SnapshotStream::Disk)
    }

    fn read_at<'py>(
        &self,
        py: Python<'py>,
        offset: u64,
        length: usize,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let inner = self.inner.clone();
        let data = py
            .allow_threads(move || inner.read_at(SnapshotStream::Disk, offset, length))
            .map_err(|e| PyIOError::new_err(e.to_string()))?;
        Ok(PyBytes::new(py, &data))
    }

    #[pyo3(signature = (size=None))]
    fn read<'py>(&self, py: Python<'py>, size: Option<usize>) -> PyResult<Bound<'py, PyBytes>> {
        let mut cursor = self.cursor.lock().unwrap();
        let total_size = self.inner.size(SnapshotStream::Disk);

        if *cursor >= total_size {
            return Ok(PyBytes::new(py, &[]));
        }

        let len = match size {
            Some(s) => std::cmp::min(s as u64, total_size - *cursor) as usize,
            None => (total_size - *cursor) as usize,
        };

        let inner = self.inner.clone();
        let start = *cursor;

        let data = py
            .allow_threads(move || inner.read_at(SnapshotStream::Disk, start, len))
            .map_err(|e| PyIOError::new_err(e.to_string()))?;

        *cursor += data.len() as u64;
        Ok(PyBytes::new(py, &data))
    }

    /// Zero-copy read into a writable Python buffer.
    ///
    /// Uses the C buffer protocol to write directly into a pre-allocated
    /// Python buffer (e.g., numpy array, bytearray) without an intermediate
    /// copy. See `tensor::numpy` for the safety-critical FFI implementation.
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

        // Release GIL for reading
        let data_res =
            py.allow_threads(move || inner.read_at(SnapshotStream::Disk, start, read_len));

        match data_res {
            Ok(data) => {
                // SAFETY: buf_info.ptr is valid for buf_info.len bytes and writable,
                // guaranteed by the successful PyObject_GetBuffer call with PY_BUF_WRITABLE.
                // data.len() <= buf_info.len because read_len was clamped above.
                unsafe {
                    tensor::numpy::copy_to_buffer(&buf_info, &data);
                }
                tensor::numpy::release_buffer(buf_info);
                *cursor += data.len() as u64;
                Ok(data.len())
            }
            Err(e) => {
                tensor::numpy::release_buffer(buf_info);
                Err(PyIOError::new_err(e.to_string()))
            }
        }
    }

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
