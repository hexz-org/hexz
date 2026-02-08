//! Rust error to Python exception conversion.
//!
//! Centralizes the mapping from Rust error types to PyO3 exception types
//! so that the binding layer doesn't need to repeat conversion logic.

use pyo3::PyErr;
use pyo3::exceptions::{PyIOError, PyValueError};

use crate::engine::OpenError;

/// Converts an `OpenError` into a Python exception.
impl From<OpenError> for PyErr {
    fn from(err: OpenError) -> PyErr {
        match err {
            OpenError::UnsupportedScheme(s) => PyValueError::new_err(s),
            OpenError::Io(s) => PyIOError::new_err(s),
            OpenError::InvalidHeader(s) => PyValueError::new_err(s),
            OpenError::InvalidS3Uri(s) => PyValueError::new_err(s),
        }
    }
}
