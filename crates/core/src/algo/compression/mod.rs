//! Compression codecs for snapshot blocks.
//!
//! Provides the `Compressor` trait and concrete implementations (LZ4, Zstd).
//! The format layer uses this trait to encode and decode blocks without
//! depending on concrete algorithms.

use hexz_common::Result;
use std::fmt::Debug;

/// Pluggable interface for block-oriented compressors.
///
/// **Architectural intent:** Allows snapshot writers and readers to operate
/// against an abstraction rather than a specific compression library, making
/// it possible to add or swap algorithms without touching the format layer.
///
/// **Constraints:** Implementations must be thread-safe and stateless or
/// internally synchronized; all methods are expected to be pure functions of
/// their inputs.
pub trait Compressor: Send + Sync + Debug {
    /// Compresses `data` into an owned buffer.
    ///
    /// **Architectural intent:** Encodes a single logical block using the
    /// compressor's native framing.
    ///
    /// **Constraints:** The caller is responsible for choosing an appropriate
    /// block size; extremely large inputs may cause excessive memory usage.
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>>;

    /// Decompresses an encoded block into a new buffer.
    ///
    /// **Architectural intent:** Reverses `compress`, returning the original
    /// block bytes or failing if corruption or format errors are detected.
    ///
    /// **Constraints:** The input must have been produced by a compatible
    /// encoder; malformed data must surface as a `Error::Compression`.
    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>>;

    /// Decompresses an encoded block into a caller-provided buffer.
    ///
    /// **Architectural intent:** Enables buffer reuse for hot paths to reduce
    /// allocation pressure.
    ///
    /// **Constraints:** Implementations may fail if `out` is too small for the
    /// decompressed payload; the return value is the number of bytes written.
    fn decompress_into(&self, data: &[u8], out: &mut [u8]) -> Result<usize>;
}

/// LZ4 compression backend.
///
/// Provides a fast, lightweight `Compressor` implementation tuned for
/// low-latency reads.
pub mod lz4;

/// Zstandard compression backend with optional dictionary support.
///
/// Used when higher compression ratios or trained dictionaries are desired.
pub mod zstd;

pub use lz4::Lz4Compressor;
pub use zstd::ZstdCompressor;

use crate::format::header::CompressionType;
use hexz_common::constants::DEFAULT_ZSTD_LEVEL;

/// Create a compressor from a [`CompressionType`] enum.
pub fn create_compressor(
    comp_type: CompressionType,
    level: Option<i32>,
    dictionary: Option<Vec<u8>>,
) -> Box<dyn Compressor> {
    match comp_type {
        CompressionType::Lz4 => Box::new(Lz4Compressor::new()),
        CompressionType::Zstd => Box::new(ZstdCompressor::new(
            level.unwrap_or(DEFAULT_ZSTD_LEVEL),
            dictionary,
        )),
    }
}

/// Parse a compression string ("lz4"/"zstd") and create a compressor + type.
pub fn create_compressor_from_str(
    s: &str,
    level: Option<i32>,
    dictionary: Option<Vec<u8>>,
) -> (Box<dyn Compressor>, CompressionType) {
    match s {
        "zstd" => (
            create_compressor(CompressionType::Zstd, level, dictionary),
            CompressionType::Zstd,
        ),
        _ => (
            create_compressor(CompressionType::Lz4, level, dictionary),
            CompressionType::Lz4,
        ),
    }
}
