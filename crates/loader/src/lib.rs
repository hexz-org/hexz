//! Rust-to-Python bridge and high-performance data loading engine for Strata.
//!
//! # Overview
//!
//! `strata-loader` serves as the primary interface between the high-performance
//! `strata-core` snapshot engine and external high-level languages, primarily
//! Python. It handles the translation of complex Rust types (like `StrataFile`)
//! into Python-friendly classes using PyO3, while maintaining the performance
//! guarantees required for machine learning workloads.
//!
//! # Architecture
//!
//! The crate is split into three distinct layers:
//!
//! - **[`engine`]**: A pure Rust abstraction layer. It provides a simplified
//!   interface for opening snapshots from various sources (Local, S3, HTTP)
//!   without requiring PyO3 dependencies.
//! - **[`py_interface`]**: The PyO3 binding layer. This module defines the
//!   `#[pyclass]` and `#[pymethods]` that are exposed to Python scripts.
//! - **[`tensor`]**: Optimized buffer management for zero-copy (or low-copy)
//!   data transfer between Rust's memory space and Python's buffer protocol
//!   (e.g., NumPy arrays).
//!
//! # Key Components
//!
//! - `StrataReader`: Synchronous file-like reader for snapshots.
//! - `AsyncStrataReader`: `asyncio`-compatible reader for high-throughput I/O.
//! - `StrataBuilder`: High-level interface for creating new snapshots.
//!
//! # Error Handling
//!
//! Errors from the core engine are mapped to Python's standard exception
//! hierarchy (e.g., `IOError`, `ValueError`) via the [`py_interface::exceptions`] module.

use pyo3::prelude::*;

pub mod engine;
pub mod py_interface;
pub mod tensor;

#[pymodule]
fn strata_loader(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Register custom exceptions
    let py = m.py();
    py_interface::exceptions::register_exceptions(py, m)?;

    // Dataset classes
    m.add_class::<py_interface::dataset::StrataReader>()?;
    m.add_class::<py_interface::async_dataset::AsyncStrataReader>()?;
    m.add_class::<py_interface::builder::StrataBuilder>()?;

    // Pack function
    m.add_function(wrap_pyfunction!(py_interface::pack::pack, m)?)?;

    // Helper functions
    m.add_function(wrap_pyfunction!(py_interface::ops::inspect, m)?)?;
    m.add_function(wrap_pyfunction!(py_interface::ops::analyze, m)?)?;
    m.add_function(wrap_pyfunction!(py_interface::ops::diff, m)?)?;
    #[cfg(feature = "signing")]
    m.add_function(wrap_pyfunction!(py_interface::ops::keygen, m)?)?;
    #[cfg(feature = "signing")]
    m.add_function(wrap_pyfunction!(py_interface::ops::sign_image, m)?)?;
    #[cfg(feature = "signing")]
    m.add_function(wrap_pyfunction!(py_interface::ops::verify_image, m)?)?;
    m.add_function(wrap_pyfunction!(py_interface::ops::snapshot_vm, m)?)?;

    // Version info functions
    m.add_function(wrap_pyfunction!(py_interface::ops::get_format_version, m)?)?;
    m.add_function(wrap_pyfunction!(
        py_interface::ops::get_min_supported_version,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(
        py_interface::ops::get_max_supported_version,
        m
    )?)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    // Note: The strata_loader function is a PyO3 module initializer that requires
    // a Python interpreter to test. The actual functionality is tested through
    // Python integration tests in python/tests/.
    //
    // These Rust unit tests verify that the module structure is correct and that
    // the code compiles properly.

    #[test]
    fn test_module_structure_compiles() {
        // This test verifies that all the modules referenced in lib.rs compile correctly.
        // If any module fails to resolve, this test will not compile.
        use crate::engine;
        use crate::tensor;
        let _ = std::mem::size_of::<engine::iterator::SnapshotIterator>();
        let _ = std::mem::size_of::<tensor::numpy::BufferInfo>();
    }

    #[test]
    fn test_engine_module_exists() {
        // Verify the engine module is accessible
        use crate::engine;
        // This will fail to compile if the module doesn't exist
        let _ = std::mem::size_of::<engine::iterator::SnapshotIterator>();
    }

    #[test]
    fn test_py_interface_module_exists() {
        // Verify the py_interface module is accessible
        use crate::py_interface;
        // This will fail to compile if the module doesn't exist
        let _ = std::mem::size_of::<py_interface::dataset::StrataReader>();
    }

    #[test]
    fn test_tensor_module_exists() {
        // Verify the tensor module is accessible
        use crate::tensor;
        // Module exists and can be referenced
        let _ = std::mem::size_of::<tensor::numpy::BufferInfo>();
    }
}
