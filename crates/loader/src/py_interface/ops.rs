//! Python-exposed helper functions for snapshot inspection, analysis,
//! signing, verification, diffing, and VM snapshotting.

use pyo3::exceptions::{PyIOError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use strata_common::sign;
use strata_core::algo::dedup::{cdc, dcam};
use strata_core::format::header::StrataHeader;
use strata_core::format::index::MasterIndex;
use strata_core::format::magic::HEADER_SIZE;
use strata_core::format::version::{
    CURRENT_VERSION, MAX_SUPPORTED_VERSION, MIN_SUPPORTED_VERSION, check_version,
    compatibility_message,
};

use super::builder::StrataBuilder;

/// Generate an Ed25519 keypair for signing snapshots.
///
/// Writes `private.key` and `public.key` into the given directory (default: current directory).
/// Returns (private_key_path, public_key_path).
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

/// Get the current format version.
#[pyfunction]
pub fn get_format_version() -> u32 {
    CURRENT_VERSION
}

/// Get the minimum supported format version.
#[pyfunction]
pub fn get_min_supported_version() -> u32 {
    MIN_SUPPORTED_VERSION
}

/// Get the maximum supported format version.
#[pyfunction]
pub fn get_max_supported_version() -> u32 {
    MAX_SUPPORTED_VERSION
}

#[pyfunction]
pub fn inspect(py: Python<'_>, path: String) -> PyResult<HashMap<String, PyObject>> {
    let mut f = File::open(&path).map_err(|e| PyIOError::new_err(e.to_string()))?;

    let mut header_bytes = [0u8; HEADER_SIZE];
    f.read_exact(&mut header_bytes)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    let header: StrataHeader =
        bincode::deserialize(&header_bytes).map_err(|e| PyValueError::new_err(e.to_string()))?;

    f.seek(SeekFrom::Start(header.index_offset))
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    let mut index_bytes = Vec::new();
    f.read_to_end(&mut index_bytes)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    let master: MasterIndex =
        bincode::deserialize(&index_bytes).map_err(|e| PyValueError::new_err(e.to_string()))?;

    let file_len = f
        .metadata()
        .map_err(|e| PyIOError::new_err(e.to_string()))?
        .len();
    let total_uncompressed = master.disk_size + master.memory_size;
    let ratio = if file_len > 0 {
        total_uncompressed as f64 / file_len as f64
    } else {
        0.0
    };

    // Check version compatibility
    let compatibility = check_version(header.version);
    let is_compatible = compatibility.is_compatible();
    let compatibility_status = match compatibility {
        strata_core::format::version::VersionCompatibility::Full => "full",
        strata_core::format::version::VersionCompatibility::Degraded => "degraded",
        strata_core::format::version::VersionCompatibility::Incompatible => "incompatible",
    };

    let mut dict: HashMap<String, PyObject> = HashMap::new();

    // Read user metadata if present
    if let (Some(offset), Some(length)) = (header.metadata_offset, header.metadata_length) {
        if length > 0 {
            f.seek(SeekFrom::Start(offset))
                .map_err(|e| PyIOError::new_err(e.to_string()))?;
            let mut meta_bytes = vec![0u8; length as usize];
            f.read_exact(&mut meta_bytes)
                .map_err(|e| PyIOError::new_err(e.to_string()))?;

            // Decode using Python's json module
            if let Ok(json) = py.import("json") {
                let bytes_obj = pyo3::types::PyBytes::new(py, &meta_bytes);
                if let Ok(user_meta) = json.call_method1("loads", (bytes_obj,)) {
                    if let Ok(user_dict) = user_meta.downcast::<pyo3::types::PyDict>() {
                        for (k, v) in user_dict {
                            if let Ok(key) = k.extract::<String>() {
                                dict.insert(key, v.into_pyobject(py)?.unbind());
                            }
                        }
                    }
                }
            }
        }
    }

    dict.insert(
        "version".to_string(),
        header.version.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "current_version".to_string(),
        CURRENT_VERSION.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "min_supported_version".to_string(),
        MIN_SUPPORTED_VERSION.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "max_supported_version".to_string(),
        MAX_SUPPORTED_VERSION.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "is_compatible".to_string(),
        <pyo3::Bound<'_, pyo3::types::PyBool> as Clone>::clone(&pyo3::types::PyBool::new(
            py,
            is_compatible,
        ))
        .unbind()
        .into(),
    );
    dict.insert(
        "compatibility_status".to_string(),
        compatibility_status.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "compatibility_message".to_string(),
        compatibility_message(header.version)
            .into_pyobject(py)?
            .unbind()
            .into(),
    );
    dict.insert(
        "block_size".to_string(),
        header.block_size.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "compression".to_string(),
        format!("{:?}", header.compression)
            .into_pyobject(py)?
            .unbind()
            .into(),
    );
    dict.insert(
        "encrypted".to_string(),
        <pyo3::Bound<'_, pyo3::types::PyBool> as Clone>::clone(&pyo3::types::PyBool::new(
            py,
            header.encryption.is_some(),
        ))
        .unbind()
        .into(),
    );
    dict.insert(
        "parent_path".to_string(),
        header.parent_path.into_pyobject(py)?.unbind(),
    );
    dict.insert(
        "disk_size".to_string(),
        master.disk_size.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "memory_size".to_string(),
        master.memory_size.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "file_size".to_string(),
        file_len.into_pyobject(py)?.unbind().into(),
    );
    dict.insert(
        "ratio".to_string(),
        ratio.into_pyobject(py)?.unbind().into(),
    );
    Ok(dict)
}

#[pyfunction]
pub fn analyze(py: Python<'_>, path: String) -> PyResult<HashMap<String, f64>> {
    let path_buf = std::path::PathBuf::from(path);

    let stats = py.allow_threads(move || -> PyResult<(u64, f64, f64)> {
        let file = File::open(&path_buf).map_err(|e| PyIOError::new_err(e.to_string()))?;
        let len = file
            .metadata()
            .map_err(|e| PyIOError::new_err(e.to_string()))?
            .len();

        let sample_size = 512 * 1024 * 1024;
        let read_len = std::cmp::min(len, sample_size);
        let mut buffer = vec![0u8; read_len as usize];

        let mut f = File::open(&path_buf).map_err(|e| PyIOError::new_err(e.to_string()))?;
        f.read_exact(&mut buffer)
            .map_err(|e| PyIOError::new_err(e.to_string()))?;

        let baseline = dcam::DedupeParams::lbfs_baseline();
        let cdc_stats = cdc::analyze_stream(&buffer[..], &baseline)
            .map_err(|e| PyIOError::new_err(e.to_string()))?;

        let c = dcam::calculate_c(cdc_stats.unique_bytes, cdc_stats.total_bytes, &baseline);
        let ratio = dcam::predict_ratio(cdc_stats.total_bytes, c, &baseline);

        Ok((cdc_stats.unique_bytes, c, ratio))
    })?;

    let mut dict = HashMap::new();
    dict.insert("unique_bytes".to_string(), stats.0 as f64);
    dict.insert("change_probability".to_string(), stats.1);
    dict.insert("predicted_ratio".to_string(), stats.2);

    Ok(dict)
}

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

    let count = len / 8;
    let size = count * 4096;

    let mut dict = HashMap::new();
    dict.insert("modified_blocks".to_string(), count);
    dict.insert("estimated_size".to_string(), size);

    Ok(dict)
}

#[pyfunction]
pub fn sign_image(image_path: String, key_path: String) -> PyResult<()> {
    let image_path = PathBuf::from(image_path);
    let key_path = PathBuf::from(key_path);

    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&image_path)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    let mut header_bytes = [0u8; HEADER_SIZE];
    f.read_exact(&mut header_bytes)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    let mut header: StrataHeader =
        bincode::deserialize(&header_bytes).map_err(|e| PyValueError::new_err(e.to_string()))?;

    f.seek(SeekFrom::Start(header.index_offset))
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    let mut index_bytes = Vec::new();
    f.read_to_end(&mut index_bytes)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    let mut hasher = Sha256::new();
    hasher.update(&index_bytes);
    let digest = hasher.finalize();

    let signature = sign::sign_digest(&key_path, &digest)
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

    let signature_offset = f
        .seek(SeekFrom::End(0))
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    f.write_all(&signature)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    header.signature_offset = Some(signature_offset);
    header.signature_length = Some(signature.len() as u32);

    f.seek(SeekFrom::Start(0))
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    f.write_all(&bincode::serialize(&header).map_err(|e| PyValueError::new_err(e.to_string()))?)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    Ok(())
}

#[pyfunction]
pub fn verify_image(image_path: String, key_path: String) -> PyResult<()> {
    let image_path = PathBuf::from(image_path);
    let key_path = PathBuf::from(key_path);

    let mut f = File::open(&image_path).map_err(|e| PyIOError::new_err(e.to_string()))?;
    let mut header_bytes = [0u8; HEADER_SIZE];
    f.read_exact(&mut header_bytes)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    let header: StrataHeader =
        bincode::deserialize(&header_bytes).map_err(|e| PyValueError::new_err(e.to_string()))?;

    let (sig_off, sig_len) = match (header.signature_offset, header.signature_length) {
        (Some(o), Some(l)) => (o, l),
        _ => return Err(PyValueError::new_err("Image is not signed")),
    };

    if sig_len != 64 {
        return Err(PyValueError::new_err("Invalid signature length"));
    }

    let mut signature = [0u8; 64];
    f.seek(SeekFrom::Start(sig_off))
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    f.read_exact(&mut signature)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    let index_len = sig_off - header.index_offset;
    f.seek(SeekFrom::Start(header.index_offset))
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    let mut index_reader = f.take(index_len);
    let mut index_bytes = Vec::new();
    index_reader
        .read_to_end(&mut index_bytes)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;

    let mut hasher = Sha256::new();
    hasher.update(&index_bytes);
    let digest = hasher.finalize();

    sign::verify_digest(&key_path, &digest, &signature)
        .map_err(|e| PyValueError::new_err(format!("Verification failed: {}", e)))?;

    Ok(())
}

fn send_qmp(
    stream: &mut UnixStream,
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

fn read_qmp(stream: &mut UnixStream) -> PyResult<serde_json::Value> {
    let mut buf = [0u8; 4096];
    let n = stream
        .read(&mut buf)
        .map_err(|e| PyIOError::new_err(e.to_string()))?;
    let s = String::from_utf8_lossy(&buf[..n]);

    for line in s.lines() {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if val.get("return").is_some() {
                return Ok(val);
            }
        }
    }
    Ok(serde_json::json!({}))
}

#[pyfunction]
pub fn snapshot_vm(
    py: Python<'_>,
    qmp_socket: String,
    base_path: String,
    overlay_path: String,
    output_path: String,
) -> PyResult<()> {
    let socket_path = PathBuf::from(qmp_socket);

    let mut stream = UnixStream::connect(&socket_path)
        .map_err(|e| PyIOError::new_err(format!("Failed to connect to QMP: {}", e)))?;

    let _ = read_qmp(&mut stream)?;
    let _ = send_qmp(&mut stream, "qmp_capabilities", None)?;
    let _ = send_qmp(&mut stream, "stop", None)?;

    let mem_dump = tempfile::NamedTempFile::new().map_err(|e| PyIOError::new_err(e.to_string()))?;
    let mem_path = mem_dump.path().to_string_lossy().to_string();

    let migrate_cmd = format!("exec:cat > {}", mem_path);
    let args = serde_json::json!({ "uri": migrate_cmd });
    let _ = send_qmp(&mut stream, "migrate", Some(args))?;

    loop {
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

    let mut builder = StrataBuilder::new(
        output_path,
        65536,
        "lz4",
        None,
        true,
        false,
        16384,
        65536,
        131072,
    )?;
    builder.merge_overlay(py, base_path, overlay_path, false)?;
    builder.add_memory_file(py, mem_path)?;
    builder.finalize()?;

    let _ = send_qmp(&mut stream, "cont", None)?;

    Ok(())
}
