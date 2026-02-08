//! LZ4 block compression for snapshot data.
//!
//! Implements the `Compressor` trait using the `lz4_flex` crate. Used for
//! fast, low-latency compression when read performance matters more than
//! maximum ratio.

use crate::algo::compression::Compressor;
use strata_common::{Result, StrataError};

#[derive(Debug, Default)]
/// LZ4-based block compressor implementation.
///
/// **Architectural intent:** Provides a fast, asymmetric compressor for
/// snapshot blocks where read latency matters more than maximum compression
/// ratio.
///
/// **Constraints:** Uses the `lz4_flex` framing that prepends the uncompressed
/// size to the payload, which is relied upon by `decompress` and
/// `decompress_into`.
pub struct Lz4Compressor;

impl Lz4Compressor {
    /// Constructs a new stateless LZ4 compressor instance.
    ///
    /// **Architectural intent:** Offers a cheap handle that satisfies the
    /// `Compressor` trait; all state is kept in local temporaries per call.
    ///
    /// **Constraints:** The type does not carry configuration; tuning must be
    /// performed by swapping implementations rather than mutating this one.
    pub fn new() -> Self {
        Self
    }
}

impl Compressor for Lz4Compressor {
    /// Compresses a block of data using LZ4 with a prepended size header.
    ///
    /// **Architectural intent:** Mirrors the format used in the snapshot
    /// writer so that decoding can inspect the leading size and allocate
    /// appropriately.
    ///
    /// **Constraints:** The caller is responsible for bounding `data` size; no
    /// streaming interface is provided.
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(lz4_flex::compress_prepend_size(data))
    }

    /// Decompresses a size-prefixed LZ4 payload into a new buffer.
    ///
    /// **Architectural intent:** Wraps `lz4_flex` and normalizes errors into
    /// the `StrataError::Compression` domain.
    ///
    /// **Constraints:** The input must have been produced by
    /// `compress_prepend_size`; truncated or malformed buffers are reported as
    /// compression failures.
    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        lz4_flex::decompress_size_prepended(data)
            .map_err(|e| StrataError::Compression(e.to_string()))
    }

    /// Decompresses a size-prefixed LZ4 payload into an existing buffer.
    ///
    /// **Architectural intent:** Avoids reallocation for hot paths where the
    /// caller can reuse decode buffers across reads.
    ///
    /// **Constraints:** The caller must ensure that `out` is large enough to
    /// hold the decompressed payload; if `data` is shorter than the LZ4 header
    /// the function fails with a compression error.
    ///
    /// **Side effects:** Writes into `out` and returns the number of bytes
    /// produced; does not shrink or reallocate the buffer.
    fn decompress_into(&self, data: &[u8], out: &mut [u8]) -> Result<usize> {
        if data.len() < 4 {
            return Err(StrataError::Compression("Data too short".into()));
        }
        lz4_flex::decompress_into(&data[4..], out)
            .map_err(|e| StrataError::Compression(e.to_string()))
    }
}
