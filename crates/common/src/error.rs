//! Unified error type and result alias for Hexz.
//!
//! Defines `Error` and the crate-wide `Result<T>` alias so that all
//! library and CLI code can report I/O, format, compression, and
//! encryption failures through a single type.

use thiserror::Error;

/// Unified error type for all Hexz library and CLI operations.
///
/// **Architectural intent:** Provides a single error surface that callers can
/// use for I/O, format, compression, encryption, and feature-negotiation
/// failures, enabling consistent reporting across crates.
///
/// **Constraints:** Variants must remain serializable as user-facing strings
/// and stable enough for log analysis; binary compatibility is not guaranteed
/// across releases.
///
/// **Side effects:** Many variants wrap underlying error types; formatting the
/// error may allocate and may expose details from lower layers that should not
/// be relied upon for programmatic decisions.
#[derive(Error, Debug)]
pub enum Error {
    /// Underlying I/O failure (file, network, or OS-level).
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),

    /// Binary serialization/deserialization failure (bincode layer).
    #[error("Serialization Error: {0}")]
    Serialization(#[from] bincode::Error),

    /// Compression or decompression failure.
    #[error("Compression Error: {0}")]
    Compression(String),

    /// Encryption or decryption failure (wrong key, corrupted ciphertext).
    #[error("Encryption Error: {0}")]
    Encryption(String),

    /// Integrity check failure — stored checksum does not match computed value.
    #[error("Corrupted Data: Checksum mismatch at block {0}")]
    Corruption(u64),

    /// Archive format violation (bad magic, unsupported version, truncation).
    #[error("Invalid Format: {0}")]
    Format(String),

    /// Block index exceeds the archive's block count.
    #[error("Block index {0} out of bounds")]
    OutOfBounds(u64),

    /// Requested feature requires a build flag or version that is not present.
    #[error("Feature Not Supported: {0}")]
    NotSupported(String),
}

/// Convenience result alias for functions that can fail with `Error`.
///
/// **Architectural intent:** Standardizes the error channel across the
/// codebase so APIs clearly communicate that failures are domain-specific
/// Hexz errors rather than arbitrary `std` errors.
///
/// **Constraints:** Callers should treat `Error` as opaque and avoid
/// depending on the exact display text; matching on variants is preferred for
/// structured handling.
///
/// **Side effects:** No additional side effects beyond those of the underlying
/// operation whose result is being wrapped.
pub type Result<T> = std::result::Result<T, Error>;
