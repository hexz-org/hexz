//! Async Python dataset class wrapping the Rust engine.
//!
//! Provides `AsyncStrataReader` (asyncio-compatible) that delegates
//! to the pure-Rust engine layer via `tokio::task::spawn_blocking`.

use pyo3::exceptions::{PyIOError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::sync::{Arc, Mutex};
use strata_core::StrataFile;
use strata_core::api::stratafile::SnapshotStream;

use crate::engine::{self, OpenConfig};

#[pyclass(module = "strata._strata_core")]
pub struct AsyncStrataReader {
    inner: Arc<StrataFile>,
    cursor: Arc<Mutex<u64>>,
}

#[pymethods]
impl AsyncStrataReader {
    #[staticmethod]
    #[pyo3(signature = (path, s3_region=None, endpoint_url=None, allow_restricted=false))]
    fn create(
        py: Python<'_>,
        path: String,
        s3_region: Option<String>,
        endpoint_url: Option<String>,
        allow_restricted: bool,
    ) -> PyResult<Bound<'_, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let core = tokio::task::spawn_blocking(move || -> PyResult<Arc<StrataFile>> {
                let config = OpenConfig {
                    path,
                    s3_region,
                    endpoint_url,
                    allow_restricted,
                };
                engine::open_snapshot(config).map_err(|e| PyIOError::new_err(e.to_string()))
            })
            .await
            .map_err(|e: tokio::task::JoinError| PyRuntimeError::new_err(e.to_string()))??;

            let reader = AsyncStrataReader {
                inner: core,
                cursor: Arc::new(Mutex::new(0)),
            };

            Ok(reader)
        })
    }

    fn size(&self) -> u64 {
        self.inner.size(SnapshotStream::Disk)
    }

    /// Read bytes. If `offset` is None, reads from current position and advances cursor.
    /// If `offset` is Some(k), reads from that position without moving the cursor.
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
                        let mut pos = cursor.lock().unwrap();
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
                let mut pos = cursor.lock().unwrap();
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

    fn tell(&self) -> u64 {
        *self.cursor.lock().unwrap()
    }

    fn __aenter__<'p>(slf: Py<Self>, py: Python<'p>) -> PyResult<Bound<'p, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(slf) })
    }

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
