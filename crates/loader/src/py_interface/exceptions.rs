//! Python exception types and error conversion for Hexz.
//!
//! This module defines custom Python exception types that map Rust errors to a structured
//! exception hierarchy. It provides clear, specific error types for different failure modes
//! and implements conversion traits to automatically map Rust errors to appropriate Python
//! exceptions.
//!
//! # Exception Hierarchy
//!
//! All Hexz exceptions inherit from `Error`, which itself inherits from Python's
//! built-in `Exception`. This allows callers to catch all Hexz-specific errors with a
//! single `except Error` clause or target specific error categories:
//!
//! ```text
//! Exception (built-in)
//! └── Error (base for all Hexz errors)
//!     ├── IOError (I/O and storage errors)
//!     │   └── NetworkError (remote storage errors)
//!     ├── FormatError (invalid file format)
//!     │   └── VersionError (version incompatibility)
//!     ├── ValidationError (parameter validation errors)
//!     ├── CompressionError (compression/decompression failures)
//!     ├── EncryptionError (cryptographic errors)
//!     └── CacheError (cache operation failures)
//! ```
//!
//! # Exception Types
//!
//! ## Error
//!
//! Base exception for all Hexz-related errors. Catch this to handle any Hexz error:
//!
//! ```python
//! try:
//!     reader = Reader("data.hxz")
//! except Error as e:
//!     print(f"Hexz error: {e}")
//! ```
//!
//! ## IOError
//!
//! I/O operation failed. Includes file system errors, permission denied, device errors.
//! Inherits from both `Error` and Python's `OSError` for compatibility.
//!
//! Common causes:
//! - File not found
//! - Permission denied
//! - Disk full
//! - Network timeout (for remote storage)
//!
//! ```python
//! try:
//!     reader = Reader("missing.hxz")
//! except IOError as e:
//!     print(f"I/O error: {e}")
//! ```
//!
//! ## NetworkError
//!
//! Network-related I/O error. Raised for failures in remote storage backends (HTTP, S3).
//!
//! Common causes:
//! - HTTP connection failure
//! - S3 authentication error
//! - DNS resolution failure
//! - Request timeout
//!
//! ```python
//! try:
//!     reader = Reader("s3://invalid-bucket/data.hxz")
//! except NetworkError as e:
//!     print(f"Network error: {e}")
//! ```
//!
//! ## FormatError
//!
//! Invalid file format or corrupted snapshot structure.
//!
//! Common causes:
//! - Wrong magic bytes (not a Hexz file)
//! - Corrupted header
//! - Invalid index structure
//! - Truncated file
//!
//! ```python
//! try:
//!     reader = Reader("not-a-snapshot.txt")
//! except FormatError as e:
//!     print(f"Format error: {e}")
//! ```
//!
//! ## VersionError
//!
//! Incompatible format version. Raised when a snapshot's version is outside the
//! supported range.
//!
//! Common causes:
//! - Snapshot created with newer version of Hexz
//! - Legacy snapshot format no longer supported
//!
//! ```python
//! try:
//!     reader = Reader("old-format.hxz")
//! except VersionError as e:
//!     print(f"Version error: {e}")
//!     print(f"File version: {e.file_version}")
//!     print(f"Supported: {e.supported_version}")
//! ```
//!
//! ## ValidationError
//!
//! Invalid parameters or constraint violations.
//!
//! Common causes:
//! - Invalid URI scheme
//! - Invalid S3 URI format
//! - Out-of-range parameter values
//! - Missing required fields
//!
//! ```python
//! try:
//!     reader = Reader("ftp://example.com/data.hxz")  # unsupported scheme
//! except ValidationError as e:
//!     print(f"Validation error: {e}")
//! ```
//!
//! ## CompressionError
//!
//! Compression or decompression failure.
//!
//! Common causes:
//! - Unsupported compression algorithm
//! - Corrupted compressed data
//! - Compression level out of range
//!
//! ```python
//! try:
//!     builder = Builder("out.hxz", compression="invalid")
//! except CompressionError as e:
//!     print(f"Compression error: {e}")
//! ```
//!
//! ## EncryptionError
//!
//! Cryptographic operation failure.
//!
//! Common causes:
//! - Wrong password
//! - Invalid encryption key
//! - Corrupted encrypted data
//! - Missing encryption parameters
//!
//! ```python
//! try:
//!     reader = Reader("encrypted.hxz", password="wrong")
//! except EncryptionError as e:
//!     print(f"Encryption error: {e}")
//! ```
//!
//! ## CacheError
//!
//! Cache operation failure.
//!
//! Common causes:
//! - Cache full (cannot evict entries)
//! - Invalid cache configuration
//! - Cache corruption
//!
//! # Error Handling Best Practices
//!
//! ## Specific Exception Handling
//!
//! Catch specific exceptions when you can handle them differently:
//!
//! ```python
//! from hexz import Reader, IOError, FormatError, VersionError
//!
//! try:
//!     reader = Reader("data.hxz")
//! except IOError as e:
//!     print(f"File not accessible: {e}")
//!     # Try alternative path
//! except FormatError as e:
//!     print(f"Invalid snapshot: {e}")
//!     # Alert user about corruption
//! except VersionError as e:
//!     print(f"Incompatible version: {e}")
//!     # Suggest upgrade
//! ```
//!
//! ## Generic Exception Handling
//!
//! Catch `Error` for general error handling:
//!
//! ```python
//! from hexz import Reader, Error
//!
//! try:
//!     reader = Reader("data.hxz")
//!     data = reader.read(4096)
//! except Error as e:
//!     logger.error(f"Hexz operation failed: {e}")
//!     return None
//! ```
//!
//! ## Preserving Tracebacks
//!
//! Always preserve exception context when re-raising:
//!
//! ```python
//! try:
//!     reader = Reader("data.hxz")
//! except Error as e:
//!     logger.error(f"Failed to open snapshot: {e}")
//!     raise  # preserves traceback
//! ```
//!
//! # Implementation Details
//!
//! This module uses PyO3's `create_exception!` macro to define exception types that
//! inherit from Python's exception hierarchy. The exceptions are defined in both Rust
//! (this file) and Python (`hexz/exceptions.py`) for rich documentation and proper
//! hierarchy in both languages.
//!
//! The `From<OpenError>` implementation automatically converts Rust errors to Python
//! exceptions when crossing the FFI boundary, ensuring consistent error handling throughout
//! the codebase.

use pyo3::exceptions::PyException;
use pyo3::{PyErr, Python, create_exception};

use crate::engine::OpenError;

// Define custom Python exception types
// These will be registered in the Python module and can be imported as:
// from hexz import Error, IOError, FormatError, etc.

create_exception!(hexz, Error, PyException);
create_exception!(hexz, IOError, Error);
create_exception!(hexz, NetworkError, IOError);
create_exception!(hexz, FormatError, Error);
create_exception!(hexz, ValidationError, Error);
create_exception!(hexz, CompressionError, Error);
create_exception!(hexz, EncryptionError, Error);
create_exception!(hexz, CacheError, Error);
create_exception!(hexz, VersionError, FormatError);

/// Register all custom exceptions with the Python module.
///
/// This should be called from the module initialization function.
pub fn register_exceptions(
    _py: Python,
    _m: &pyo3::Bound<'_, pyo3::types::PyModule>,
) -> pyo3::PyResult<()> {
    // Note: We don't add exceptions here because they're already defined
    // in the Python layer (hexz/exceptions.py). The create_exception! macro
    // creates Rust types that we can use to raise those exceptions.
    //
    // If we wanted to define them purely in Rust, we would do:
    // m.add("Error", py.get_type_bound::<Error>())?;
    // But since we have rich Python docstrings and a proper hierarchy,
    // we just use the Python-defined exceptions.
    Ok(())
}

/// Converts an `OpenError` into a Python exception.
///
/// Automatically maps Rust errors from the engine layer to appropriate Python exception
/// types. This conversion happens transparently when Rust functions return `Result` types
/// across the PyO3 boundary.
///
/// # Mapping Rules
///
/// - `OpenError::UnsupportedScheme` → `ValidationError`
/// - `OpenError::Io` → `IOError`
/// - `OpenError::InvalidHeader` → `FormatError`
/// - `OpenError::InvalidS3Uri` → `ValidationError`
///
/// # Example
///
/// ```rust,ignore
/// #[pyfunction]
/// fn open_snapshot(path: String) -> PyResult<Reader> {
///     let config = OpenConfig { path, ... };
///     // If this returns Err(OpenError::Io(...)), it's automatically
///     // converted to Python IOError via this trait
///     let inner = engine::open_snapshot(config)?;
///     Ok(Reader { inner, ... })
/// }
/// ```
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

/// Helper trait for converting generic errors to Error
pub trait IntoError {
    fn into_hexz_error(self) -> PyErr;
}

impl<E: std::fmt::Display> IntoError for E {
    fn into_hexz_error(self) -> PyErr {
        Error::new_err(format!("{}", self))
    }
}

// Note: Tests for this module require Python runtime and are covered by
// the Python integration test suite (189 tests in crates/loader/tests/).
// Rust unit tests for PyO3 code cannot run without linking to Python.
