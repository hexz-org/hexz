//! Rust error to Python exception conversion.
//!
//! Centralizes the mapping from Rust error types to PyO3 exception types
//! so that the binding layer doesn't need to repeat conversion logic.
//!
//! This module defines custom Python exceptions that map to the exception
//! hierarchy defined in strata.exceptions.

use pyo3::exceptions::PyException;
use pyo3::{PyErr, Python, create_exception};

use crate::engine::OpenError;

// Define custom Python exception types
// These will be registered in the Python module and can be imported as:
// from strata import StrataError, IOError, FormatError, etc.

create_exception!(strata, StrataError, PyException);
create_exception!(strata, IOError, StrataError);
create_exception!(strata, NetworkError, IOError);
create_exception!(strata, FormatError, StrataError);
create_exception!(strata, ValidationError, StrataError);
create_exception!(strata, CompressionError, StrataError);
create_exception!(strata, EncryptionError, StrataError);
create_exception!(strata, CacheError, StrataError);
create_exception!(strata, VersionError, FormatError);

/// Register all custom exceptions with the Python module.
///
/// This should be called from the module initialization function.
pub fn register_exceptions(
    _py: Python,
    _m: &pyo3::Bound<'_, pyo3::types::PyModule>,
) -> pyo3::PyResult<()> {
    // Note: We don't add exceptions here because they're already defined
    // in the Python layer (strata/exceptions.py). The create_exception! macro
    // creates Rust types that we can use to raise those exceptions.
    //
    // If we wanted to define them purely in Rust, we would do:
    // m.add("StrataError", py.get_type_bound::<StrataError>())?;
    // But since we have rich Python docstrings and a proper hierarchy,
    // we just use the Python-defined exceptions.
    Ok(())
}

/// Converts an `OpenError` into a Python exception.
///
/// Maps Rust errors to appropriate Python exception types from our
/// custom hierarchy.
impl From<OpenError> for PyErr {
    fn from(err: OpenError) -> PyErr {
        match err {
            OpenError::UnsupportedScheme(s) => {
                ValidationError::new_err(format!("Unsupported URI scheme: {}", s))
            }
            OpenError::Io(s) => IOError::new_err(format!("I/O error: {}", s)),
            OpenError::InvalidHeader(s) => {
                FormatError::new_err(format!("Invalid file format: {}", s))
            }
            OpenError::InvalidS3Uri(s) => {
                ValidationError::new_err(format!("Invalid S3 URI: {}", s))
            }
        }
    }
}

/// Helper trait for converting std::io::Error to our custom IOError
pub trait IntoPyIOError {
    fn into_py_io_error(self) -> PyErr;
}

impl IntoPyIOError for std::io::Error {
    fn into_py_io_error(self) -> PyErr {
        IOError::new_err(format!("I/O error: {}", self))
    }
}

/// Helper trait for converting generic errors to StrataError
pub trait IntoStrataError {
    fn into_strata_error(self) -> PyErr;
}

impl<E: std::fmt::Display> IntoStrataError for E {
    fn into_strata_error(self) -> PyErr {
        StrataError::new_err(format!("{}", self))
    }
}
