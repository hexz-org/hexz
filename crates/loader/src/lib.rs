//! Strata Python bindings.
//!
//! Architecture:
//! - `engine/` — Pure Rust logic (no PyO3 dependency)
//! - `py_interface/` — PyO3 binding layer
//! - `tensor/` — Zero-copy buffer operations

use pyo3::prelude::*;

pub mod engine;
pub mod py_interface;
pub mod tensor;

#[pymodule]
fn _strata_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
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
    m.add_function(wrap_pyfunction!(py_interface::ops::keygen, m)?)?;
    m.add_function(wrap_pyfunction!(py_interface::ops::sign_image, m)?)?;
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
