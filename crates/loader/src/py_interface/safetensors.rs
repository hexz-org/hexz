//! Python bindings for safetensors native import/export.
//!
//! Exposes `store_safetensors` and `extract_safetensors` as Python functions
//! that operate without PyTorch — tensor bytes are transferred directly between
//! files at the OS level.

use hexz_ops::safetensors::{
    SafetensorsStoreConfig, extract_safetensors as ops_extract, store_safetensors as ops_store,
};
use pyo3::exceptions::PyIOError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::PathBuf;

/// Convert a `.safetensors` file to a Hexz archive.
///
/// No PyTorch required. Reads tensor bytes directly from the source file.
/// If `base` is provided, blocks identical to the parent archive are stored
/// as lightweight references rather than new data (deduplication of frozen tensors).
///
/// Returns a dict with:
/// - `tensors`: number of tensors stored
/// - `total_bytes`: total uncompressed tensor data size
/// - `stored_bytes`: physical bytes written to the output file
/// - `elapsed_secs`: wall-clock time in seconds
#[pyfunction]
#[pyo3(signature = (input, output, base=None, compression="zstd", block_size=65536, show_progress=true))]
pub fn store_safetensors(
    py: Python<'_>,
    input: String,
    output: String,
    base: Option<String>,
    compression: &str,
    block_size: u32,
    show_progress: bool,
) -> PyResult<PyObject> {
    let config = SafetensorsStoreConfig {
        input: PathBuf::from(input),
        output: PathBuf::from(output),
        base: base.map(PathBuf::from),
        compression: compression.to_string(),
        block_size,
        show_progress,
    };

    let summary = py
        .allow_threads(move || ops_store(config))
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    let dict = PyDict::new(py);
    dict.set_item("tensors", summary.tensors)?;
    dict.set_item("total_bytes", summary.total_bytes)?;
    dict.set_item("stored_bytes", summary.stored_bytes)?;
    dict.set_item("xor_delta_tensors", summary.xor_delta_tensors)?;
    dict.set_item("elapsed_secs", summary.elapsed_secs)?;
    Ok(dict.into())
}

/// Reconstruct a `.safetensors` file from a Hexz archive.
///
/// If `tensor` is provided, only the raw bytes for that tensor are written
/// (no file header). If omitted, a complete `.safetensors` file is produced.
#[pyfunction]
#[pyo3(signature = (input, output, tensor=None))]
pub fn extract_safetensors(
    py: Python<'_>,
    input: String,
    output: String,
    tensor: Option<String>,
) -> PyResult<()> {
    let input = PathBuf::from(input);
    let output = PathBuf::from(output);
    let tensor_name = tensor.clone();

    py.allow_threads(move || {
        ops_extract(&input, &output, tensor_name.as_deref())
            .map_err(|e| PyIOError::new_err(e.to_string()))
    })
}
