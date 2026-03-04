use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3::exceptions::{PyIOError, PyValueError};
use hexz_core::api::file::{Archive as CoreArchive, ArchiveStream};
use hexz_core::api::manifest::{ArchiveManifest, FileEntry};
use hexz_store::local::MmapBackend;
use hexz_ops::pack::{pack_archive, PackConfig};
use std::sync::Arc;
use std::path::PathBuf;

#[pyclass]
struct Archive {
    inner: Arc<CoreArchive>,
    manifest: Option<ArchiveManifest>,
}

#[pymethods]
impl Archive {
    #[new]
    #[pyo3(signature = (path, cache_size=None))]
    fn new(path: String, cache_size: Option<String>) -> PyResult<Self> {
        let backend = Arc::new(MmapBackend::new(path.as_ref())
            .map_err(|e| PyIOError::new_err(format!("Failed to open archive: {}", e)))?);
        
        let cache_capacity = if let Some(s) = cache_size {
            parse_size(&s).map_err(PyValueError::new_err)?
        } else {
            0 // Default
        };

        let inner = CoreArchive::open_with_cache(
            backend,
            None, // Auto-detect encryptor (needs password if encrypted)
            if cache_capacity > 0 { Some(cache_capacity) } else { None },
            None, // Default prefetch
        ).map_err(|e| PyIOError::new_err(format!("Failed to initialize archive: {}", e)))?;

        let manifest = inner.metadata.as_ref().and_then(|m| {
            serde_json::from_slice::<ArchiveManifest>(m).ok()
        });

        Ok(Self {
            inner,
            manifest,
        })
    }

    fn namelist(&self) -> Vec<String> {
        self.manifest.as_ref()
            .map(|m| m.files.iter().map(|f| f.path.clone()).collect())
            .unwrap_or_default()
    }

    fn read(&self, py: Python<'_>, name: String) -> PyResult<PyObject> {
        let entry = self.manifest.as_ref()
            .and_then(|m| m.find_file(&name))
            .ok_or_else(|| PyValueError::new_err(format!("File not found: {}", name)))?;

        let data = self.inner.read_at(ArchiveStream::Main, entry.offset, entry.size as usize)
            .map_err(|e| PyIOError::new_err(e.to_string()))?;

        Ok(PyBytes::new(py, &data).into())
    }

    fn open(&self, name: String) -> PyResult<File> {
        let entry = self.manifest.as_ref()
            .and_then(|m| m.find_file(&name))
            .ok_or_else(|| PyValueError::new_err(format!("File not found: {}", name)))?;

        Ok(File {
            archive: self.inner.clone(),
            entry: entry.clone(),
            pos: 0,
        })
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __exit__(&self, _exc_type: PyObject, _exc_value: PyObject, _traceback: PyObject) {}
}

#[pyclass]
struct File {
    archive: Arc<CoreArchive>,
    entry: FileEntry,
    pos: u64,
}

#[pymethods]
impl File {
    #[pyo3(signature = (n=None))]
    fn read(&mut self, py: Python<'_>, n: Option<i64>) -> PyResult<PyObject> {
        let size = self.entry.size;
        let remaining = size.saturating_sub(self.pos);
        let to_read = match n {
            Some(n) if n >= 0 => std::cmp::min(n as u64, remaining) as usize,
            _ => remaining as usize,
        };

        if to_read == 0 {
            return Ok(PyBytes::new(py, &[]).into());
        }

        let data = self.archive.read_at(ArchiveStream::Main, self.entry.offset + self.pos, to_read)
            .map_err(|e| PyIOError::new_err(e.to_string()))?;
        
        self.pos += to_read as u64;
        Ok(PyBytes::new(py, &data).into())
    }

    #[pyo3(signature = (offset, whence=0))]
    fn seek(&mut self, offset: i64, whence: i32) -> PyResult<u64> {
        let new_pos = match whence {
            0 => offset, // SEEK_SET
            1 => self.pos as i64 + offset, // SEEK_CUR
            2 => self.entry.size as i64 + offset, // SEEK_END
            _ => return Err(PyValueError::new_err("invalid whence")),
        };

        if new_pos < 0 {
            return Err(PyValueError::new_err("negative seek offset"));
        }

        self.pos = std::cmp::min(new_pos as u64, self.entry.size);
        Ok(self.pos)
    }

    fn tell(&self) -> u64 {
        self.pos
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __exit__(&self, _exc_type: PyObject, _exc_value: PyObject, _traceback: PyObject) {}
}

#[pyfunction]
#[pyo3(signature = (input, output, base=None, compression="lz4".to_string()))]
fn pack(input: String, output: String, base: Option<String>, compression: String) -> PyResult<()> {
    let config = PackConfig {
        input: PathBuf::from(input),
        output: PathBuf::from(output),
        base: base.map(PathBuf::from),
        compression,
        use_dcam: true,
        ..Default::default()
    };

    pack_archive(config, None::<fn(u64, u64)>)
        .map_err(|e| PyIOError::new_err(format!("Packing failed: {}", e)))?;
    
    Ok(())
}

#[pyfunction]
fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[pymodule]
fn hexz(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Archive>()?;
    m.add_class::<File>()?;
    m.add_function(wrap_pyfunction!(pack, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}

fn parse_size(s: &str) -> Result<usize, String> {
    let s = s.trim().to_uppercase();
    let (num_str, suffix) = if let Some(idx) = s.find(|c: char| !c.is_numeric() && c != '.') {
        (&s[..idx], &s[idx..])
    } else {
        (s.as_str(), "")
    };

    let num: f64 = num_str.parse().map_err(|_| "Invalid size number")?;
    let multiplier = match suffix {
        "K" | "KB" => 1024.0,
        "M" | "MB" => 1024.0 * 1024.0,
        "G" | "GB" => 1024.0 * 1024.0 * 1024.0,
        "T" | "TB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        "" => 1.0,
        _ => return Err("Invalid size suffix".to_string()),
    };

    Ok((num * multiplier) as usize)
}
