//! Asynchronous S3 storage backend with embedded Tokio runtime.
//!
//! This module provides an async-capable S3 storage backend that wraps the
//! `rust-s3` async client in an embedded Tokio runtime. This allows the backend
//! to present a synchronous `StorageBackend` interface while leveraging async
//! I/O internally for efficient concurrent S3 operations.
//!
//! # Architecture
//!
//! The [`AsyncS3Backend`] embeds a Tokio runtime (`Arc<Runtime>`) and uses
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
//! strata-core = { version = "*", features = ["s3"] }
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
//! Compared to the synchronous [`S3Backend`](super::sync::S3Backend):
//! - **Pros**: Lower memory overhead for many concurrent connections, supports custom endpoints
//! - **Cons**: Slight latency overhead from async runtime context switching
//!
//! # When to Use This Backend
//!
//! Prefer [`AsyncS3Backend`] when:
//! - You already have an async runtime in your application
//! - You need to access S3-compatible storage with custom endpoints
//! - You need to manage many concurrent S3 connections efficiently
//!
//! Prefer synchronous [`S3Backend`](super::sync::S3Backend) when:
//! - Your application is purely synchronous
//! - You want to minimize dependencies and complexity
//! - Using AWS S3 (no custom endpoint needed)
//!
//! # Examples
//!
//! ## AWS S3
//!
//! ```no_run
//! # #[cfg(feature = "s3")]
//! # {
//! use strata_core::store::s3::AsyncS3Backend;
//! use strata_core::store::StorageBackend;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = AsyncS3Backend::new(
//!     "my-snapshots".to_string(),
//!     "prod/snapshot-001.st".to_string(),
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
//!
//! ## MinIO (S3-Compatible)
//!
//! ```no_run
//! # #[cfg(feature = "s3")]
//! # {
//! use strata_core::store::s3::AsyncS3Backend;
//! use strata_core::store::StorageBackend;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = AsyncS3Backend::new(
//!     "snapshots".to_string(),
//!     "data.st".to_string(),
//!     "us-east-1".to_string(), // Region name (can be arbitrary for MinIO)
//!     Some("https://minio.example.com:9000".to_string()) // Custom endpoint
//! )?;
//!
//! let data = backend.read_exact(0, 512)?;
//! # Ok(())
//! # }
//! # }
//! ```

#[cfg(feature = "s3")]
use crate::store::StorageBackend;
#[cfg(feature = "s3")]
use bytes::Bytes;
#[cfg(feature = "s3")]
use s3::bucket::Bucket;
#[cfg(feature = "s3")]
use s3::creds::Credentials;
#[cfg(feature = "s3")]
use s3::region::Region;
#[cfg(feature = "s3")]
use std::io::{Error, ErrorKind};
#[cfg(feature = "s3")]
use std::str::FromStr;
#[cfg(feature = "s3")]
use std::sync::Arc;
#[cfg(feature = "s3")]
use strata_common::{Result, StrataError};
#[cfg(feature = "s3")]
use tokio::runtime::Runtime;

/// Asynchronous S3 storage backend with embedded Tokio runtime.
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
/// use strata_core::store::s3::AsyncS3Backend;
/// use strata_core::store::StorageBackend;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // AWS S3
/// let backend = AsyncS3Backend::new(
///     "my-snapshots".to_string(),
///     "snapshot.st".to_string(),
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
pub struct AsyncS3Backend {
    /// The S3 bucket client used for async API operations.
    bucket: Box<Bucket>,
    /// The object key (path) within the bucket.
    key: String,
    /// The total object size in bytes, obtained via HEAD request during construction.
    len: u64,
    /// Embedded Tokio runtime for executing async operations synchronously.
    runtime: Arc<Runtime>,
}

#[cfg(feature = "s3")]
impl AsyncS3Backend {
    /// Creates a new async S3 backend with optional custom endpoint support.
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
    /// - `key`: The object key within the bucket (e.g., `"prod/snapshot-001.st"`)
    /// - `region_name`: AWS region or arbitrary name for custom endpoints (e.g., `"us-east-1"`)
    /// - `endpoint`: Optional custom endpoint URL for S3-compatible storage
    ///   (e.g., `Some("https://minio.example.com:9000".to_string())`)
    ///
    /// # Returns
    ///
    /// - `Ok(AsyncS3Backend)` if the object is accessible and credentials are valid
    /// - `Err(StrataError::Io)` if validation or network operations fail
    ///
    /// # Errors
    ///
    /// Common error conditions:
    /// - **Runtime creation failure**: Cannot initialize Tokio runtime (rare)
    /// - **Invalid region** (`ErrorKind::InvalidInput`): Region string is not recognized
    ///   (only relevant when `endpoint` is `None`)
    /// - **Missing credentials** (`ErrorKind::PermissionDenied`): No valid AWS credentials
    ///   found in environment, config files, or IAM role
    /// - **Bucket error**: Bucket name is invalid or configuration is incorrect
    /// - **Object not found** (`ErrorKind::NotFound`, HTTP 404): The specified object
    ///   does not exist in the bucket
    /// - **Access denied** (HTTP 403): IAM policy does not grant `s3:GetObject` permission
    /// - **Network failure**: DNS resolution failure, connection timeout, or service unavailable
    /// - **Missing Content-Length** (`ErrorKind::InvalidData`): S3 response does not include
    ///   object size (extremely rare)
    /// - **Negative Content-Length**: Malformed S3 response (should never occur)
    ///
    /// # Custom Endpoints
    ///
    /// When `endpoint` is `Some(url)`, the backend uses `Region::Custom` with the
    /// provided URL. This enables access to S3-compatible storage systems:
    /// - **MinIO**: `https://minio.example.com:9000`
    /// - **DigitalOcean Spaces**: `https://nyc3.digitaloceanspaces.com`
    /// - **Wasabi**: `https://s3.wasabisys.com`
    ///
    /// When `endpoint` is `None`, the backend uses the standard AWS S3 endpoint for
    /// the specified region.
    ///
    /// # Performance
    ///
    /// This constructor performs:
    /// - One runtime initialization (~1-5ms)
    /// - One async HEAD request to S3 (~50-200ms)
    ///
    /// Total latency: ~50-200ms
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(feature = "s3")]
    /// # {
    /// use strata_core::store::s3::AsyncS3Backend;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // AWS S3
    /// let aws = AsyncS3Backend::new(
    ///     "my-snapshots".to_string(),
    ///     "snapshot.st".to_string(),
    ///     "us-east-1".to_string(),
    ///     None
    /// )?;
    ///
    /// // MinIO
    /// let minio = AsyncS3Backend::new(
    ///     "snapshots".to_string(),
    ///     "data.st".to_string(),
    ///     "local".to_string(), // Arbitrary region name
    ///     Some("https://minio.example.com:9000".to_string())
    /// )?;
    ///
    /// // Error handling
    /// match AsyncS3Backend::new(
    ///     "nonexistent-bucket".to_string(),
    ///     "missing.st".to_string(),
    ///     "us-west-2".to_string(),
    ///     None
    /// ) {
    ///     Ok(_) => println!("Success"),
    ///     Err(e) => eprintln!("Failed to open S3 backend: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    pub fn new(
        bucket_name: String,
        key: String,
        region_name: String,
        endpoint: Option<String>,
    ) -> Result<Self> {
        let runtime = Runtime::new().map_err(StrataError::Io)?;

        let region = if let Some(ep) = endpoint {
            Region::Custom {
                region: region_name,
                endpoint: ep,
            }
        } else {
            Region::from_str(&region_name).map_err(|e| {
                StrataError::Io(Error::new(
                    ErrorKind::InvalidInput,
                    format!("Invalid region: {}", e),
                ))
            })?
        };

        let credentials = Credentials::default().map_err(|e| {
            StrataError::Io(Error::new(
                ErrorKind::PermissionDenied,
                format!("Missing credentials: {}", e),
            ))
        })?;

        let bucket = Bucket::new(&bucket_name, region, credentials)
            .map_err(|e| StrataError::Io(Error::other(format!("Bucket error: {}", e))))?
            .with_path_style();

        // Perform HEAD request to get size and validate access
        let (head, code) = runtime
            .block_on(async { bucket.head_object(&key).await })
            .map_err(|e| StrataError::Io(Error::other(format!("S3 Head error: {}", e))))?;

        if code != 200 {
            return Err(StrataError::Io(Error::new(
                ErrorKind::NotFound,
                format!("S3 object not found or error: {}", code),
            )));
        }

        let len = head.content_length.ok_or_else(|| {
            StrataError::Io(Error::new(ErrorKind::InvalidData, "Missing Content-Length"))
        })?;

        if len < 0 {
            return Err(StrataError::Io(Error::new(
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
impl StorageBackend for AsyncS3Backend {
    /// Reads exactly `len` bytes starting at `offset` using an async S3 range GET request.
    ///
    /// This method sends an async S3 `GetObject` request with a byte-range parameter
    /// to fetch only the requested data. The async operation is executed on the embedded
    /// Tokio runtime using `block_on()`, presenting a synchronous interface.
    ///
    /// # Parameters
    ///
    /// - `offset`: Starting byte offset from the beginning of the object (0-indexed)
    /// - `len`: Number of bytes to read
    ///
    /// # Returns
    ///
    /// - `Ok(Bytes)`: A buffer containing exactly `len` bytes
    /// - `Err(StrataError::Io)`: If the S3 request fails or response is invalid
    ///
    /// # Errors
    ///
    /// Common error conditions:
    /// - **Network timeout**: Request timeout or connection failure
    /// - **S3 error codes**:
    ///   - `404 Not Found`: Object was deleted between construction and read
    ///   - `403 Forbidden`: IAM permissions changed or credentials expired
    ///   - `500/503 Server Error`: S3 service unavailable (transient)
    /// - **Unexpected EOF** (`ErrorKind::UnexpectedEof`): S3 returns fewer bytes than
    ///   requested (should not occur unless object was truncated)
    /// - **Invalid range**: Requesting beyond object boundaries (rare, validated by S3)
    ///
    /// # Performance
    ///
    /// - **Latency**: 50-200ms per request (network RTT + S3 processing + runtime overhead)
    /// - **Throughput**: Up to 100MB/s per connection (scales with concurrent requests)
    /// - **Cost**: ~$0.0004 per 1000 requests + data transfer fees
    ///
    /// For best performance:
    /// - Use a block cache to minimize redundant S3 requests
    /// - Prefer larger reads (64KB-1MB) to amortize request overhead
    /// - Consider S3 Transfer Acceleration for cross-region access
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(feature = "s3")]
    /// # {
    /// use strata_core::store::s3::AsyncS3Backend;
    /// use strata_core::store::StorageBackend;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = AsyncS3Backend::new(
    ///     "my-snapshots".to_string(),
    ///     "snapshot.st".to_string(),
    ///     "us-east-1".to_string(),
    ///     None
    /// )?;
    ///
    /// // Read first 512 bytes
    /// let header = backend.read_exact(0, 512)?;
    /// assert_eq!(header.len(), 512);
    ///
    /// // Read 1MB block at offset 10MB
    /// let block = backend.read_exact(10 * 1024 * 1024, 1024 * 1024)?;
    /// assert_eq!(block.len(), 1024 * 1024);
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    fn read_exact(&self, offset: u64, len: usize) -> Result<Bytes> {
        let end = offset + len as u64 - 1;

        self.runtime.block_on(async {
            let response_data = self
                .bucket
                .get_object_range(&self.key, offset, Some(end))
                .await
                .map_err(|e| StrataError::Io(Error::other(format!("S3 Read error: {}", e))))?;

            let code = response_data.status_code();
            if code != 200 && code != 206 {
                return Err(StrataError::Io(Error::other(format!(
                    "S3 error code: {}",
                    code
                ))));
            }

            let data = response_data.as_slice();

            if data.len() != len {
                return Err(StrataError::Io(Error::new(
                    ErrorKind::UnexpectedEof,
                    format!("Expected {} bytes, got {}", len, data.len()),
                )));
            }

            Ok(Bytes::copy_from_slice(data))
        })
    }

    /// Returns the total S3 object size in bytes.
    ///
    /// This value is obtained from the `Content-Length` header during the initial
    /// async HEAD request and is cached for the lifetime of the backend. The object
    /// is assumed to be immutable; if the object is deleted or replaced externally,
    /// this value will not be updated and subsequent reads may fail.
    ///
    /// # Returns
    ///
    /// The object size in bytes as of the time `AsyncS3Backend::new()` was called.
    ///
    /// # Performance
    ///
    /// This method is a simple field access with no network I/O (O(1)).
    fn len(&self) -> u64 {
        self.len
    }
}
