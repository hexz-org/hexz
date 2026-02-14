//! S3 storage backend with embedded Tokio runtime.
//!
//! This module provides an S3 storage backend that wraps the `rust-s3` async
//! client in an embedded Tokio runtime. This allows the backend to present a
//! synchronous `StorageBackend` interface while leveraging async I/O internally
//! for efficient concurrent S3 operations.
//!
//! # Architecture
//!
//! The [`S3Backend`] embeds a Tokio runtime (`Arc<Runtime>`) and uses
//! `runtime.block_on()` to execute async S3 operations synchronously. This design:
//! - Maintains compatibility with the synchronous `StorageBackend` trait
//! - Enables efficient connection pooling and concurrent requests via `rust-s3`
//! - Provides async benefits (low memory overhead per connection) without requiring
//!   callers to use async/await
//!
//! # Feature Gate
//!
//! This module is only available when the `s3` feature is enabled:
//! ```toml
//! [dependencies]
//! hexz-core = { version = "*", features = ["s3"] }
//! ```
//!
//! # Custom Endpoints (S3-Compatible Storage)
//!
//! This backend supports S3-compatible object storage systems (MinIO, DigitalOcean
//! Spaces, Wasabi, etc.) via the optional `endpoint` parameter. When provided,
//! the backend uses a custom endpoint URL instead of AWS S3.
//!
//! # Thread Safety
//!
//! The backend is fully thread-safe (`Send + Sync`):
//! - The `rust-s3` async client is designed for concurrent use
//! - The `Arc<Runtime>` is shared safely across threads
//! - Multiple threads can call `read_exact()` concurrently without coordination
//!
//! # Performance Characteristics
//!
//! - **Latency**: 50-200ms per request (network RTT + S3 processing)
//! - **Throughput**: Up to 100MB/s per connection (scales with parallel requests)
//! - **Runtime overhead**: ~100µs per request for `block_on()` context switch
//!
//! # Examples
//!
//! ```no_run
//! # #[cfg(feature = "s3")]
//! # {
//! use hexz_core::store::s3::S3Backend;
//! use hexz_core::store::StorageBackend;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = S3Backend::new(
//!     "my-snapshots".to_string(),
//!     "prod/snapshot-001.hxz".to_string(),
//!     "us-east-1".to_string(),
//!     None // Use AWS endpoint
//! )?;
//!
//! let data = backend.read_exact(0, 512)?;
//! assert_eq!(data.len(), 512);
//! # Ok(())
//! # }
//! # }
//! ```

#[cfg(feature = "s3")]
use crate::store::StorageBackend;
#[cfg(feature = "s3")]
use bytes::Bytes;
#[cfg(feature = "s3")]
use hexz_common::{Error, Result};
#[cfg(feature = "s3")]
use s3::bucket::Bucket;
#[cfg(feature = "s3")]
use s3::creds::Credentials;
#[cfg(feature = "s3")]
use s3::region::Region;
#[cfg(feature = "s3")]
use std::io::{Error as IoError, ErrorKind};
#[cfg(feature = "s3")]
use std::str::FromStr;
#[cfg(feature = "s3")]
use std::sync::Arc;
#[cfg(feature = "s3")]
use tokio::runtime::Runtime;

/// S3 storage backend with embedded Tokio runtime.
///
/// This backend wraps an async `rust-s3` `Bucket` client and Tokio `Runtime` to
/// provide synchronous `StorageBackend` operations while leveraging async I/O
/// internally. It supports both AWS S3 and S3-compatible storage via custom endpoints.
///
/// # Examples
///
/// ```no_run
/// # #[cfg(feature = "s3")]
/// # {
/// use hexz_core::store::s3::S3Backend;
/// use hexz_core::store::StorageBackend;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // AWS S3
/// let backend = S3Backend::new(
///     "my-snapshots".to_string(),
///     "snapshot.hxz".to_string(),
///     "us-east-1".to_string(),
///     None
/// )?;
///
/// // Read 4KB at offset 8192
/// let data = backend.read_exact(8192, 4096)?;
/// assert_eq!(data.len(), 4096);
/// # Ok(())
/// # }
/// # }
/// ```
#[cfg(feature = "s3")]
#[derive(Debug)]
pub struct S3Backend {
    bucket: Box<Bucket>,
    key: String,
    len: u64,
    runtime: Arc<Runtime>,
}

#[cfg(feature = "s3")]
impl S3Backend {
    /// Creates a new S3 backend with optional custom endpoint support.
    ///
    /// This constructor:
    /// 1. Creates a Tokio runtime for executing async operations
    /// 2. Parses the region and optionally constructs a custom endpoint
    /// 3. Loads AWS credentials from the default credential chain
    /// 4. Creates a `Bucket` client with path-style addressing
    /// 5. Sends an async HEAD request to verify object existence and fetch size
    ///
    /// # Parameters
    ///
    /// - `bucket_name`: The S3 bucket name (e.g., `"my-snapshots"`)
    /// - `key`: The object key within the bucket (e.g., `"prod/snapshot-001.hxz"`)
    /// - `region_name`: AWS region or arbitrary name for custom endpoints (e.g., `"us-east-1"`)
    /// - `endpoint`: Optional custom endpoint URL for S3-compatible storage
    ///   (e.g., `Some("https://minio.example.com:9000".to_string())`)
    pub fn new(
        bucket_name: String,
        key: String,
        region_name: String,
        endpoint: Option<String>,
    ) -> Result<Self> {
        let runtime = Runtime::new().map_err(Error::Io)?;

        let region = if let Some(ep) = endpoint {
            Region::Custom {
                region: region_name,
                endpoint: ep,
            }
        } else {
            Region::from_str(&region_name).map_err(|e| {
                Error::Io(IoError::new(
                    ErrorKind::InvalidInput,
                    format!("Invalid region: {}", e),
                ))
            })?
        };

        let credentials = Credentials::default().map_err(|e| {
            Error::Io(IoError::new(
                ErrorKind::PermissionDenied,
                format!("Missing credentials: {}", e),
            ))
        })?;

        let bucket = Bucket::new(&bucket_name, region, credentials)
            .map_err(|e| Error::Io(IoError::other(format!("Bucket error: {}", e))))?
            .with_path_style();

        // Perform HEAD request to get size and validate access (with 30s timeout)
        let (head, code) = runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(30), bucket.head_object(&key))
                .await
                .map_err(|_| {
                    Error::Io(IoError::new(
                        ErrorKind::TimedOut,
                        "S3 connection timeout after 30 seconds",
                    ))
                })?
                .map_err(|e| Error::Io(IoError::other(format!("S3 Head error: {}", e))))
        })?;

        if code != 200 {
            return Err(Error::Io(IoError::new(
                ErrorKind::NotFound,
                format!("S3 object not found or error: {}", code),
            )));
        }

        let len = head.content_length.ok_or_else(|| {
            Error::Io(IoError::new(
                ErrorKind::InvalidData,
                "Missing Content-Length",
            ))
        })?;

        if len < 0 {
            return Err(Error::Io(IoError::new(
                ErrorKind::InvalidData,
                "Negative Content-Length",
            )));
        }

        Ok(Self {
            bucket: Box::new(bucket),
            key,
            len: len as u64,
            runtime: Arc::new(runtime),
        })
    }
}

#[cfg(feature = "s3")]
impl StorageBackend for S3Backend {
    fn read_exact(&self, offset: u64, len: usize) -> Result<Bytes> {
        let end = offset + len as u64 - 1;

        self.runtime.block_on(async {
            let response_data = tokio::time::timeout(
                std::time::Duration::from_secs(60),
                self.bucket.get_object_range(&self.key, offset, Some(end)),
            )
            .await
            .map_err(|_| {
                Error::Io(IoError::new(
                    ErrorKind::TimedOut,
                    "S3 read timeout after 60 seconds",
                ))
            })?
            .map_err(|e| Error::Io(IoError::other(format!("S3 Read error: {}", e))))?;

            let code = response_data.status_code();
            if code != 200 && code != 206 {
                return Err(Error::Io(IoError::other(format!(
                    "S3 error code: {}",
                    code
                ))));
            }

            let data = response_data.as_slice();

            if data.len() != len {
                return Err(Error::Io(IoError::new(
                    ErrorKind::UnexpectedEof,
                    format!("Expected {} bytes, got {}", len, data.len()),
                )));
            }

            Ok(Bytes::copy_from_slice(data))
        })
    }

    fn len(&self) -> u64 {
        self.len
    }
}
