//! PyO3 wrapper for checkpoint loading.
//!
//! Exposes `load_checkpoint` as a Python function that performs all I/O,
//! decompression, and XOR delta reconstruction in Rust with the GIL released,
//! returning raw tensor bytes for conversion to torch tensors in Python.

use hexz_ops::checkpoint;
use pyo3::exceptions::PyIOError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use std::path::PathBuf;

/// Load tensor bytes and scalars from a hexz checkpoint.
///
/// All heavy I/O (decompression, XOR delta reconstruction, parent chain
/// resolution) happens in Rust with the GIL released. Returns a dict:
///
/// - `"tensors"`: `dict[str, bytes]` — raw tensor data
/// - `"tensor_meta"`: `dict[str, dict]` — dtype, shape per tensor
/// - `"scalars"`: `dict[str, dict]` — scalar values with type info
///
/// Python's `checkpoint.load()` wraps each bytes buffer as a torch tensor.
#[pyfunction]
#[pyo3(signature = (path, keys=None))]
pub fn load_checkpoint(
    py: Python<'_>,
    path: String,
    keys: Option<Vec<String>>,
) -> PyResult<PyObject> {
    let path_buf = PathBuf::from(&path);
    let keys_ref = keys.as_deref();

    // Run all Rust I/O with the GIL released
    let result = py
        .allow_threads(move || checkpoint::load_checkpoint(&path_buf, keys_ref))
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    // Build the Python dict (GIL held for PyObject construction)
    let outer = PyDict::new(py);

    // tensors: dict[str, bytes]
    let tensors_dict = PyDict::new(py);
    // tensor_meta: dict[str, dict]
    let meta_dict = PyDict::new(py);

    for td in &result.tensors {
        let py_bytes = PyBytes::new(py, &td.data);
        tensors_dict.set_item(&td.name, py_bytes)?;

        let entry = PyDict::new(py);
        entry.set_item("dtype", &td.dtype)?;
        let shape_list = PyList::new(py, &td.shape)?;
        entry.set_item("shape", shape_list)?;
        meta_dict.set_item(&td.name, entry)?;
    }

    // scalars: dict[str, dict] with "type" and "value"
    let scalars_dict = PyDict::new(py);
    for (name, info) in &result.scalars {
        let entry = PyDict::new(py);
        entry.set_item("type", &info.scalar_type)?;
        // Convert serde_json::Value to Python
        let py_value = json_value_to_py(py, &info.value)?;
        entry.set_item("value", py_value)?;
        scalars_dict.set_item(name, entry)?;
    }

    outer.set_item("tensors", tensors_dict)?;
    outer.set_item("tensor_meta", meta_dict)?;
    outer.set_item("scalars", scalars_dict)?;

    Ok(outer.into())
}

/// Convert a serde_json::Value to a Python object.
fn json_value_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<PyObject> {
    match value {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(b) => Ok((*b).into_pyobject(py)?.to_owned().into_any().unbind()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_pyobject(py)?.into_any().unbind())
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_pyobject(py)?.into_any().unbind())
            } else {
                Ok(py.None())
            }
        }
        serde_json::Value::String(s) => Ok(s.as_str().into_pyobject(py)?.into_any().unbind()),
        serde_json::Value::Array(arr) => {
            let items: Vec<PyObject> = arr
                .iter()
                .map(|v| json_value_to_py(py, v))
                .collect::<PyResult<_>>()?;
            Ok(PyList::new(py, items)?.into_any().unbind())
        }
        serde_json::Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, json_value_to_py(py, v)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}
