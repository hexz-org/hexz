//! Synchronous Amazon S3 storage backend.
//!
//! This module implements the `StorageBackend` interface for objects stored in
//! Amazon S3 or S3-compatible object storage systems using synchronous/blocking
//! I/O. It uses the `rust-s3` crate's blocking client to perform range GET
//! requests, enabling efficient random access to cloud-stored snapshots without
//! downloading entire objects.
//!
//! # Architecture
//!
//! The [`S3Backend`] wraps a `rust-s3` `Bucket` client and caches the object
//! size obtained via a HEAD request during construction. All reads use S3's
//! byte-range GET API to fetch only the requested data.
//!
//! # Thread Safety
//!
//! The backend is fully thread-safe (`Send + Sync`) because the underlying
//! `Bucket` client is designed for concurrent use with internal connection pooling.
//!
//! # Performance Characteristics
//!
//! - **Latency**: 50-200ms per request (depends on region and network)
//! - **Throughput**: Up to 100MB/s per connection (S3 scales with parallel requests)
//! - **Cost**: ~$0.0004 per 1000 GET requests + data transfer fees
//!
//! # When to Use This Backend
//!
//! Prefer this backend over [`AsyncS3Backend`](super::async_client::AsyncS3Backend) when:
//! - Your application is purely synchronous
//! - You want to minimize dependencies (no Tokio runtime required)
//! - Simplicity is more important than async I/O efficiency
//!
//! # Examples
//!
//! ```no_run
//! use strata_core::store::s3::S3Backend;
//! use strata_core::store::StorageBackend;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = S3Backend::new(
//!     "my-snapshots".to_string(),
//!     "prod/snapshot-001.st".to_string(),
//!     "us-east-1".to_string()
//! )?;
//!
//! // Read first 512 bytes
//! let header = backend.read_exact(0, 512)?;
//! assert_eq!(header.len(), 512);
//! # Ok(())
//! # }
//! ```

use crate::store::StorageBackend;
use bytes::Bytes;
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::region::Region;
use std::io::{Error, ErrorKind};
use std::str::FromStr;
use strata_common::{Result, StrataError};

/// Synchronous S3 storage backend for accessing objects via blocking I/O.
///
/// This backend wraps a `rust-s3` `Bucket` client and performs synchronous S3
/// API operations (HEAD, GET with byte ranges) to implement the `StorageBackend`
/// trait. It validates credentials, checks object existence during construction,
/// and maintains a connection pool for efficient repeated requests.
///
/// # Examples
///
/// ```no_run
/// use strata_core::store::s3::S3Backend;
/// use strata_core::store::StorageBackend;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = S3Backend::new(
///     "my-snapshots".to_string(),
///     "prod/snapshot-001.st".to_string(),
///     "us-east-1".to_string()
/// )?;
///
/// println!("Snapshot size: {} bytes", backend.len());
///
/// let data = backend.read_exact(0, 4096)?;
/// assert_eq!(data.len(), 4096);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct S3Backend {
    /// The S3 bucket client used for API operations.
    bucket: Bucket,
    /// The object key (path) within the bucket (e.g., "prod/snapshot-001.st").
    key: String,
    /// The total object size in bytes, obtained via HEAD request during construction.
    len: u64,
}

impl S3Backend {
    /// Creates a new S3 backend by initializing the client and validating object access.
    ///
    /// This constructor performs the following steps:
    /// 1. Parses the region string into a `Region` enum
    /// 2. Loads AWS credentials from the default credential chain
    /// 3. Creates a `Bucket` client with path-style addressing
    /// 4. Sends a HEAD request to verify object existence and fetch size
    /// 5. Caches the object size for the lifetime of the backend
    ///
    /// # Parameters
    ///
    /// - `bucket_name`: The S3 bucket name (e.g., `"my-snapshots"`)
    /// - `key`: The object key within the bucket (e.g., `"prod/2024/snapshot-001.st"`)
    /// - `region_name`: AWS region identifier (e.g., `"us-east-1"`, `"eu-west-1"`)
    ///
    /// # Returns
    ///
    /// - `Ok(S3Backend)` if the object is accessible and credentials are valid
    /// - `Err(StrataError::Io)` if validation or network operations fail
    ///
    /// # Errors
    ///
    /// Common error conditions:
    /// - **Invalid region** (`ErrorKind::InvalidInput`): Region string is not recognized
    ///   (e.g., `"invalid-region"`)
    /// - **Missing credentials** (`ErrorKind::PermissionDenied`): No valid AWS credentials
    ///   found in environment, config files, or IAM role
    /// - **Bucket error**: Bucket name is invalid or region configuration is incorrect
    /// - **Object not found** (`ErrorKind::NotFound`, HTTP 404): The specified object
    ///   does not exist in the bucket
    /// - **Access denied** (HTTP 403): IAM policy does not grant `s3:GetObject` permission
    /// - **Network failure**: DNS resolution failure, connection timeout, or S3 service unavailable
    /// - **Missing Content-Length** (`ErrorKind::InvalidData`): S3 response does not include
    ///   object size (extremely rare)
    /// - **Negative Content-Length**: Malformed S3 response (should never occur)
    ///
    /// # Credential Chain
    ///
    /// Credentials are loaded in the following order:
    /// 1. Environment variables: `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`
    /// 2. AWS credentials file: `~/.aws/credentials`
    /// 3. IAM instance profile (EC2) or ECS task role
    ///
    /// # Performance
    ///
    /// This constructor performs one synchronous HEAD request to S3, which typically
    /// takes 50-200ms depending on region and network latency.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use strata_core::store::s3::S3Backend;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Standard AWS S3
    /// let backend = S3Backend::new(
    ///     "my-snapshots".to_string(),
    ///     "prod/snapshot-001.st".to_string(),
    ///     "us-east-1".to_string()
    /// )?;
    ///
    /// // Error handling
    /// match S3Backend::new(
    ///     "nonexistent-bucket".to_string(),
    ///     "missing.st".to_string(),
    ///     "us-west-2".to_string()
    /// ) {
    ///     Ok(_) => println!("Success"),
    ///     Err(e) => eprintln!("Failed to open S3 backend: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(bucket_name: String, key: String, region_name: String) -> Result<Self> {
        let region = Region::from_str(&region_name).map_err(|e| {
            StrataError::Io(Error::new(
                ErrorKind::InvalidInput,
                format!("Invalid region: {}", e),
            ))
        })?;

        let credentials = Credentials::default().map_err(|e| {
            StrataError::Io(Error::new(
                ErrorKind::PermissionDenied,
                format!("Missing credentials: {}", e),
            ))
        })?;

        let bucket = Bucket::new(&bucket_name, region, credentials)
            .map_err(|e| StrataError::Io(Error::other(format!("Bucket error: {}", e))))?
            .with_path_style();

        let (head, code) = bucket
            .head_object_blocking(&key)
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
            bucket,
            key,
            len: len as u64,
        })
    }
}

impl StorageBackend for S3Backend {
    /// Reads exactly `len` bytes starting at `offset` using an S3 range GET request.
    ///
    /// This method sends a synchronous S3 `GetObject` request with a byte-range
    /// parameter to fetch only the requested data. The S3 API returns a 206 Partial
    /// Content response (or 200 if range is full object).
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
    /// - **Latency**: 50-200ms per request (network RTT + S3 processing)
    /// - **Throughput**: Up to 100MB/s per connection (can scale with concurrent requests)
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
    /// use strata_core::store::s3::S3Backend;
    /// use strata_core::store::StorageBackend;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = S3Backend::new(
    ///     "my-snapshots".to_string(),
    ///     "snapshot.st".to_string(),
    ///     "us-east-1".to_string()
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
    /// ```
    fn read_exact(&self, offset: u64, len: usize) -> Result<Bytes> {
        let end = offset + len as u64 - 1;

        let response_data = self
            .bucket
            .get_object_range_blocking(&self.key, offset, Some(end))
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
    }

    /// Returns the total S3 object size in bytes.
    ///
    /// This value is obtained from the `Content-Length` header during the initial
    /// HEAD request and is cached for the lifetime of the backend. The object is
    /// assumed to be immutable; if the object is deleted or replaced externally,
    /// this value will not be updated and subsequent reads may fail.
    ///
    /// # Returns
    ///
    /// The object size in bytes as of the time `S3Backend::new()` was called.
    ///
    /// # Performance
    ///
    /// This method is a simple field access with no network I/O (O(1)).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use strata_core::store::s3::S3Backend;
    /// use strata_core::store::StorageBackend;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = S3Backend::new(
    ///     "my-snapshots".to_string(),
    ///     "snapshot.st".to_string(),
    ///     "us-east-1".to_string()
    /// )?;
    ///
    /// let size = backend.len();
    /// println!("Object size: {} bytes ({} MB)", size, size / 1024 / 1024);
    /// # Ok(())
    /// # }
    /// ```
    fn len(&self) -> u64 {
        self.len
    }
}
