//! Pure Rust engine layer for Strata snapshot access.
//!
//! This module contains all non-PyO3 logic for opening, reading, and
//! iterating over Strata snapshots. It is designed to be consumed by
//! both the Python bindings and any other Rust code that needs snapshot
//! access without pulling in PyO3.

pub mod iterator;
pub mod shuffle;

use std::sync::Arc;
use strata_common::constants::DEFAULT_ZSTD_LEVEL;
use strata_core::StrataFile;
use strata_core::algo::compression::{Compressor, lz4::Lz4Compressor, zstd::ZstdCompressor};
use strata_core::api::stratafile::SnapshotStream;
use strata_core::format::header::{CompressionType, StrataHeader};
use strata_core::format::magic::HEADER_SIZE;
use strata_core::store::StorageBackend;

/// Errors that can occur when opening a snapshot.
#[derive(Debug)]
pub enum OpenError {
    /// The path/URI scheme is not recognized.
    UnsupportedScheme(String),
    /// I/O error during backend creation or header read.
    Io(String),
    /// The snapshot header is invalid or corrupt.
    InvalidHeader(String),
    /// Invalid S3 URI format.
    InvalidS3Uri(String),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::UnsupportedScheme(s) => write!(f, "Unsupported scheme: {}", s),
            OpenError::Io(s) => write!(f, "I/O error: {}", s),
            OpenError::InvalidHeader(s) => write!(f, "Invalid header: {}", s),
            OpenError::InvalidS3Uri(s) => write!(f, "Invalid S3 URI: {}", s),
        }
    }
}

impl std::error::Error for OpenError {}

/// Configuration for opening a Strata snapshot.
///
/// This struct aggregates all parameters required to locate and authenticate
/// access to a snapshot, whether it resides on a local disk or a remote
/// cloud provider.
pub struct OpenConfig {
    /// Path or URI to the snapshot file.
    ///
    /// Supports:
    /// - Local paths: `/path/to/snap.st`
    /// - HTTP/HTTPS: `https://example.com/snap.st`
    /// - S3 URIs: `s3://bucket-name/key/path.st`
    pub path: String,

    /// AWS Region to use for S3 requests.
    ///
    /// Defaults to `us-east-1` if not specified. Only used if `path`
    /// starts with `s3://`.
    pub s3_region: Option<String>,

    /// Custom endpoint URL for S3-compatible storage (e.g., MinIO, Ceph).
    ///
    /// If provided, this overrides the default AWS S3 endpoint.
    pub endpoint_url: Option<String>,

    /// Security flag to allow connections to restricted/internal IP ranges.
    ///
    /// If `false` (default), the loader will refuse to connect to private
    /// networks (RFC 1918) when using HTTP or S3 backends to prevent
    /// SSRF (Server-Side Request Forgery) attacks in hosted environments.
    pub allow_restricted: bool,
}

/// Opens a StrataFile from a path or URI.
///
/// Supports local files, HTTP(S) URLs, and S3 URIs.
/// This is pure Rust with no PyO3 dependency.
pub fn open_snapshot(config: OpenConfig) -> Result<StrataFile, OpenError> {
    let backend: Arc<dyn StorageBackend> = if config.path.starts_with("http://")
        || config.path.starts_with("https://")
    {
        Arc::new(
            strata_core::store::http::HttpBackend::new(
                config.path.clone(),
                config.allow_restricted,
            )
            .map_err(|e| OpenError::Io(e.to_string()))?,
        )
    } else if config.path.starts_with("s3://") {
        let remainder = &config.path[5..];
        let parts: Vec<&str> = remainder.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(OpenError::InvalidS3Uri(
                "Expected s3://bucket/key".to_string(),
            ));
        }
        let bucket = parts[0].to_string();
        let key = parts[1].to_string();
        let region = config.s3_region.unwrap_or_else(|| "us-east-1".to_string());

        Arc::new(
            strata_core::store::s3::AsyncS3Backend::new(bucket, key, region, config.endpoint_url)
                .map_err(|e| OpenError::Io(e.to_string()))?,
        )
    } else {
        Arc::new(
            strata_core::store::local::FileBackend::new(std::path::Path::new(&config.path))
                .map_err(|e| OpenError::Io(e.to_string()))?,
        )
    };

    let header_bytes = backend
        .read_exact(0, HEADER_SIZE)
        .map_err(|e| OpenError::Io(e.to_string()))?;

    let header: StrataHeader =
        bincode::deserialize(&header_bytes).map_err(|e| OpenError::InvalidHeader(e.to_string()))?;

    let compressor: Box<dyn Compressor> = match header.compression {
        CompressionType::Lz4 => Box::new(Lz4Compressor::new()),
        CompressionType::Zstd => Box::new(ZstdCompressor::new(DEFAULT_ZSTD_LEVEL, None)),
    };

    StrataFile::new(backend, compressor, None).map_err(|e| OpenError::Io(e.to_string()))
}

/// Returns the size of a specific stream in the snapshot.
pub fn stream_size(snap: &StrataFile, stream: SnapshotStream) -> u64 {
    snap.size(stream)
}

/// Reads bytes from a specific stream at a given offset.
pub fn read_stream(
    snap: &StrataFile,
    stream: SnapshotStream,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, OpenError> {
    snap.read_at(stream, offset, length)
        .map_err(|e| OpenError::Io(e.to_string()))
}
