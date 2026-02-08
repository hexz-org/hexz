//! Python binding for `core::ops::pack`.
//!
//! Exposes the core packing logic to Python, allowing snapshots to be
//! created directly from Python without shelling out to the CLI.

use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use std::path::PathBuf;
use strata_core::ops::pack::{PackConfig, pack_snapshot};

/// Pack disk and/or memory images into a Strata archive.
///
/// This function wraps `core::ops::pack::pack_snapshot` for Python callers.
///
/// Args:
///     output: Output archive path (.st).
///     disk: Optional path to disk image.
///     memory: Optional path to memory dump.
///     compression: Compression algorithm ("lz4" or "zstd").
///     block_size: Block size in bytes.
///     encrypt: Enable encryption.
///     password: Encryption password (required if encrypt=True).
///     cdc: Enable content-defined chunking.
///     min_chunk: Minimum CDC chunk size.
///     avg_chunk: Average CDC chunk size.
///     max_chunk: Maximum CDC chunk size.
#[allow(clippy::too_many_arguments)]
#[pyfunction]
#[pyo3(signature = (
    output,
    disk=None,
    memory=None,
    compression="lz4",
    block_size=65536,
    encrypt=false,
    password=None,
    cdc=false,
    min_chunk=16384,
    avg_chunk=65536,
    max_chunk=131072
))]
pub fn pack(
    py: Python<'_>,
    output: String,
    disk: Option<String>,
    memory: Option<String>,
    compression: &str,
    block_size: u32,
    encrypt: bool,
    password: Option<String>,
    cdc: bool,
    min_chunk: u32,
    avg_chunk: u32,
    max_chunk: u32,
) -> PyResult<()> {
    if encrypt && password.is_none() {
        return Err(PyValueError::new_err(
            "Password is required when encryption is enabled",
        ));
    }

    let config = PackConfig {
        disk: disk.map(PathBuf::from),
        memory: memory.map(PathBuf::from),
        output: PathBuf::from(output),
        compression: compression.to_string(),
        encrypt,
        password,
        train_dict: false,
        block_size,
        cdc_enabled: cdc,
        min_chunk,
        avg_chunk,
        max_chunk,
    };

    // Release the GIL during the potentially long-running pack operation
    py.allow_threads(move || {
        pack_snapshot(config, None::<fn(u64, u64)>).map_err(|e| PyIOError::new_err(e.to_string()))
    })
}
