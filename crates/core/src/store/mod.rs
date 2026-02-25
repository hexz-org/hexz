//! Storage backends for snapshot byte access.
//!
//! Implements the `StorageBackend` trait for local files, HTTP (blocking and
//! async), memory-mapped files, and S3-compatible object storage. Higher
//! layers use these backends to fetch raw bytes without caring about the
//! transport.

use bytes::Bytes;
use hexz_common::Result;
use std::fmt::Debug;

/// Abstract interface for all byte-addressable snapshot storage backends.
///
/// **Architectural intent:** Decouples the snapshot decoding logic from the
/// physical storage medium so that files, HTTP endpoints, memory maps, or
/// object stores can be swapped without changing higher layers.
///
/// **Constraints:** Implementations must be thread-safe (`Send + Sync`) and
/// provide stable, random-access reads over an immutable byte region whose
/// length does not change during the lifetime of the backend.
///
/// **Side effects:** Individual calls may perform disk or network I/O and may
/// block the calling thread; callers are responsible for scheduling behavior.
pub trait StorageBackend: Send + Sync + Debug {
    /// Reads exactly `len` bytes starting at `offset` from the underlying store.
    ///
    /// **Architectural intent:** Provides the minimal primitive needed by the
    /// format layer to fetch headers, indices, and compressed blocks.
    ///
    /// **Constraints:** `offset + len` must not exceed `self.len()`; callers
    /// should treat short reads or I/O errors as fatal for the current
    /// operation.
    ///
    /// **Side effects:** May perform synchronous I/O and allocate a new
    /// `Bytes` buffer per call; repeated small reads can be expensive.
    fn read_exact(&self, offset: u64, len: usize) -> Result<Bytes>;

    /// Returns the total logical size of the underlying snapshot in bytes.
    ///
    /// **Architectural intent:** Allows upper layers to validate range
    /// requests and compute offsets into the header and index regions.
    ///
    /// **Constraints:** The length is assumed to be immutable for the lifetime
    /// of the backend; changing it concurrently is undefined behavior.
    fn len(&self) -> u64;

    /// Returns `true` if the underlying store has zero length.
    ///
    /// **Architectural intent:** Convenience helper to avoid repeated `len()`
    /// comparisons in call sites that need to quickly reject empty inputs.
    ///
    /// **Constraints:** Equivalent to `self.len() == 0`; implementations
    /// should not override the default semantics.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
