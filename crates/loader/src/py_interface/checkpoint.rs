//! PyO3 wrappers for checkpoint save and load.
//!
//! Exposes `save_checkpoint` and `load_checkpoint` as Python functions.
//! All heavy I/O (compression, decompression, XOR delta) happens in Rust.
//! The GIL is held during save (rayon workers don't need it) and released
//! during load via `py.allow_threads()`.

use hexz_ops::checkpoint;
use pyo3::exceptions::PyIOError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};
use std::collections::HashMap;
use std::path::PathBuf;

/// Save tensors and scalars as a hexz checkpoint.
///
/// All compression, XOR delta encoding, and I/O happen in Rust. The GIL
/// is held throughout (needed for buffer protocol access), but rayon worker
/// threads run freely for parallel compression.
///
/// Args:
///     path: Output .hxz file path.
///     tensors: List of (name, buffer, dtype, shape) tuples.
///     scalars: Dict of {name: {"type": str, "value": Any}}.
///     compression: "lz4" or "zstd".
///     compression_level: Optional compression level override.
///     block_size: Block size in bytes.
///     parent: Optional parent .hxz path for XOR delta.
///     base_tensors: Optional dict {name: buffer} with reconstructed parent bytes.
///     message: Optional message stored in metadata.
///     num_workers: Rayon thread count (0 = all CPUs).
#[pyfunction]
#[pyo3(signature = (path, tensors, scalars, compression="zstd", compression_level=None, block_size=131072, parent=None, base_tensors=None, message=None, num_workers=0))]
#[allow(clippy::too_many_arguments)]
pub fn save_checkpoint(
    path: String,
    tensors: Bound<'_, PyList>,
    scalars: Bound<'_, PyDict>,
    compression: &str,
    compression_level: Option<i32>,
    block_size: u32,
    parent: Option<String>,
    base_tensors: Option<Bound<'_, PyDict>>,
    message: Option<String>,
    num_workers: usize,
) -> PyResult<()> {
    // Acquire all tensor buffer pointers (GIL held, buffers stay alive)
    let mut tensor_guards = Vec::new();
    let mut tensor_specs_raw = Vec::new();

    for item in tensors.iter() {
        let tuple: Bound<'_, PyTuple> = item.downcast_into()?;
        let name: String = tuple.get_item(0)?.extract()?;
        let buf_obj = tuple.get_item(1)?;
        let dtype: String = tuple.get_item(2)?.extract()?;
        let shape: Vec<usize> = tuple.get_item(3)?.extract()?;

        let buf_info = crate::tensor::numpy::acquire_readable_buffer(&buf_obj)?;
        let ptr = buf_info.ptr;
        let len = buf_info.len;
        tensor_guards.push(buf_info); // keep buffer alive

        tensor_specs_raw.push((name, ptr, len, dtype, shape));
    }

    // Acquire base tensor buffers if provided
    let mut base_guards = Vec::new();
    let mut base_map_raw: Vec<(String, *const u8, usize)> = Vec::new();

    if let Some(ref base_dict) = base_tensors {
        for (key, value) in base_dict.iter() {
            let name: String = key.extract()?;
            let buf_info = crate::tensor::numpy::acquire_readable_buffer(&value)?;
            let ptr = buf_info.ptr;
            let len = buf_info.len;
            base_guards.push(buf_info);
            base_map_raw.push((name, ptr, len));
        }
    }

    // Build TensorWriteSpec slices from raw pointers
    // SAFETY: buffer guards are alive, GIL is held, pointers are valid
    let tensor_specs: Vec<checkpoint::TensorWriteSpec<'_>> = tensor_specs_raw
        .iter()
        .map(|(name, ptr, len, dtype, shape)| {
            let data = unsafe { std::slice::from_raw_parts(*ptr, *len) };
            let element_size = checkpoint::dtype_element_size(dtype);
            checkpoint::TensorWriteSpec {
                name: name.clone(),
                data,
                dtype: dtype.clone(),
                shape: shape.clone(),
                element_size,
            }
        })
        .collect();

    // Build base tensors map
    let base_tensors_map: Option<HashMap<String, &[u8]>> = if base_map_raw.is_empty() {
        None
    } else {
        let mut map = HashMap::new();
        for (name, ptr, len) in &base_map_raw {
            let data = unsafe { std::slice::from_raw_parts(*ptr, *len) };
            map.insert(name.clone(), data);
        }
        Some(map)
    };

    // Parse scalars from Python dict
    let scalars_map = parse_scalars_dict(&scalars)?;

    // Build config
    let config = checkpoint::SaveCheckpointConfig {
        path: PathBuf::from(&path),
        compression: compression.to_string(),
        compression_level,
        block_size,
        parent: parent.map(PathBuf::from),
        message,
        num_workers,
        base_tensors: base_tensors_map,
    };

    // Run save (GIL held, rayon workers run freely)
    checkpoint::save_checkpoint(&tensor_specs, &scalars_map, &config)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    Ok(())
}

/// Parse Python scalars dict into Rust HashMap.
fn parse_scalars_dict(
    scalars: &Bound<'_, PyDict>,
) -> PyResult<HashMap<String, checkpoint::ScalarInfo>> {
    let mut map = HashMap::new();
    for (key, value) in scalars.iter() {
        let name: String = key.extract()?;
        let entry: Bound<'_, PyDict> = value.downcast_into()?;
        let scalar_type: String = entry.get_item("type")?.unwrap().extract()?;
        let py_value = entry.get_item("value")?.unwrap();
        let json_value = py_to_json_value(&py_value)?;
        map.insert(
            name,
            checkpoint::ScalarInfo {
                scalar_type,
                value: json_value,
            },
        );
    }
    Ok(map)
}

/// Convert a Python object to serde_json::Value.
fn py_to_json_value(obj: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if obj.is_none() {
        Ok(serde_json::Value::Null)
    } else if let Ok(b) = obj.extract::<bool>() {
        Ok(serde_json::Value::Bool(b))
    } else if let Ok(i) = obj.extract::<i64>() {
        Ok(serde_json::json!(i))
    } else if let Ok(f) = obj.extract::<f64>() {
        Ok(serde_json::json!(f))
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(serde_json::Value::String(s))
    } else {
        // Fallback: convert via str()
        let s = obj.str()?.to_string();
        Ok(serde_json::Value::String(s))
    }
}

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
