//! Asynchronous HTTP storage backend with embedded Tokio runtime.
//!
//! This module provides an async-capable HTTP storage backend that wraps the
//! `reqwest` async client in an embedded Tokio runtime. This allows the backend
//! to present a synchronous `StorageBackend` interface while leveraging async
//! I/O internally for efficient concurrent operations.
//!
//! # Architecture
//!
//! The [`AsyncHttpBackend`] embeds a Tokio runtime (`Arc<Runtime>`) and uses
//! `runtime.block_on()` to execute async operations synchronously. This design:
//! - Maintains compatibility with the synchronous `StorageBackend` trait
//! - Enables efficient connection pooling and concurrent requests via `reqwest`
//! - Provides async benefits (low memory overhead per connection) without requiring
//!   callers to use async/await
//!
//! # Feature Gate
//!
//! This module is only available when the `async-http` feature is enabled:
//! ```toml
//! [dependencies]
//! strata-core = { version = "*", features = ["async-http"] }
//! ```
//!
//! # Thread Safety
//!
//! The backend is fully thread-safe (`Send + Sync`):
//! - The `reqwest::Client` is designed for concurrent use
//! - The `Arc<Runtime>` is shared safely across threads
//! - Multiple threads can call `read_exact()` concurrently without coordination
//!
//! # Performance Characteristics
//!
//! - **Latency**: 50-200ms per request (network RTT + server processing)
//! - **Throughput**: Limited by network bandwidth and server capabilities
//! - **Connection pooling**: Automatic via `reqwest::Client`
//! - **Runtime overhead**: ~100µs per request for `block_on()` context switch
//!
//! Compared to the synchronous [`HttpBackend`](super::sync::HttpBackend):
//! - **Pros**: Lower memory overhead for many concurrent connections
//! - **Cons**: Slight latency overhead from async runtime context switching
//!
//! # When to Use This Backend
//!
//! Prefer [`AsyncHttpBackend`] when:
//! - You already have an async runtime in your application
//! - You need to manage many concurrent HTTP connections efficiently
//! - Your application is async and you want to avoid blocking thread pools
//!
//! Prefer synchronous [`HttpBackend`](super::sync::HttpBackend) when:
//! - Your application is purely synchronous
//! - You want to minimize dependencies and complexity
//! - Latency is critical and you want to avoid runtime overhead
//!
//! # Error Handling
//!
//! All HTTP errors are wrapped in `StrataError::Io`. Common errors:
//! - **Connection timeout**: Network unreachable or server overloaded
//! - **DNS resolution failure**: Domain name cannot be resolved
//! - **HTTP error codes**: 404 (Not Found), 500 (Server Error), etc.
//! - **Missing Content-Length**: Server does not provide file size
//! - **Unexpected EOF**: Server returns fewer bytes than requested
//!
//! # Security
//!
//! This backend uses the same URL validation as the synchronous backend:
//! - Blocks access to localhost and private networks by default
//! - Set `allow_restricted: true` only in trusted environments
//! - See [`validate_url`](crate::store::utils::validate_url) for details
//!
//! # Examples
//!
//! ```no_run
//! # #[cfg(feature = "async-http")]
//! # {
//! use strata_core::store::http::AsyncHttpBackend;
//! use strata_core::store::StorageBackend;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Open a remote snapshot
//! let backend = AsyncHttpBackend::new(
//!     "https://cdn.example.com/snapshots/data.st".to_string(),
//!     false // block restricted IPs
//! )?;
//!
//! println!("Snapshot size: {} bytes", backend.len());
//!
//! // Read first 512 bytes
//! let header = backend.read_exact(0, 512)?;
//! assert_eq!(header.len(), 512);
//! # Ok(())
//! # }
//! # }
//! ```

#[cfg(feature = "async-http")]
use crate::store::StorageBackend;
#[cfg(feature = "async-http")]
use crate::store::utils::validate_url;
#[cfg(feature = "async-http")]
use bytes::Bytes;
#[cfg(feature = "async-http")]
use reqwest::Client;
#[cfg(feature = "async-http")]
use std::io::{Error, ErrorKind};
#[cfg(feature = "async-http")]
use std::sync::Arc;
#[cfg(feature = "async-http")]
use strata_common::{Result, StrataError};
#[cfg(feature = "async-http")]
use tokio::runtime::Runtime;

/// Asynchronous HTTP storage backend with embedded Tokio runtime.
///
/// This backend wraps an async `reqwest::Client` and Tokio `Runtime` to provide
/// synchronous `StorageBackend` operations while leveraging async I/O internally.
/// It validates URLs for security, maintains a connection pool, and performs
/// range requests to fetch specific byte ranges.
///
/// # Examples
///
/// ```no_run
/// # #[cfg(feature = "async-http")]
/// # {
/// use strata_core::store::http::AsyncHttpBackend;
/// use strata_core::store::StorageBackend;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = AsyncHttpBackend::new(
///     "https://example.com/snapshot.st".to_string(),
///     false
/// )?;
///
/// // Read 4KB at offset 8192
/// let data = backend.read_exact(8192, 4096)?;
/// assert_eq!(data.len(), 4096);
/// # Ok(())
/// # }
/// # }
/// ```
#[cfg(feature = "async-http")]
#[derive(Debug)]
pub struct AsyncHttpBackend {
    /// The validated HTTP/HTTPS URL of the snapshot file.
    url: String,
    /// HTTP client with connection pooling and async capabilities.
    client: Client,
    /// Total file size in bytes, obtained via HEAD request during construction.
    len: u64,
    /// Embedded Tokio runtime for executing async operations synchronously.
    runtime: Arc<Runtime>,
}

#[cfg(feature = "async-http")]
impl AsyncHttpBackend {
    /// Creates a new async HTTP backend by validating the URL and fetching file metadata.
    ///
    /// This constructor:
    /// 1. Validates the URL for security (blocks restricted IPs unless allowed)
    /// 2. Creates a Tokio runtime for executing async operations
    /// 3. Sends an async HEAD request to verify the server and fetch file size
    /// 4. Extracts the `Content-Length` header to determine snapshot size
    ///
    /// # Parameters
    ///
    /// - `url`: The HTTP/HTTPS URL of the snapshot file
    /// - `allow_restricted`: If `false`, blocks access to localhost and private networks
    ///
    /// # Returns
    ///
    /// - `Ok(AsyncHttpBackend)` if the URL is valid and the server is reachable
    /// - `Err(StrataError::Io)` if validation fails, runtime creation fails, or network fails
    ///
    /// # Errors
    ///
    /// Common error conditions:
    /// - **Invalid URL**: Malformed URL or unsupported scheme (not HTTP/HTTPS)
    /// - **Restricted IP**: URL resolves to localhost or private network (when `allow_restricted: false`)
    /// - **Runtime creation failure**: Cannot initialize Tokio runtime (rare)
    /// - **Network failure**: DNS resolution failure, connection timeout, server unreachable
    /// - **HTTP error**: Server returns non-2xx status code
    /// - **Missing Content-Length**: Server does not provide `Content-Length` header
    ///
    /// # Security
    ///
    /// Set `allow_restricted: false` (recommended) to prevent SSRF attacks by blocking
    /// requests to internal networks. Only use `allow_restricted: true` in trusted
    /// environments.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(feature = "async-http")]
    /// # {
    /// use strata_core::store::http::AsyncHttpBackend;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Public URL (safe)
    /// let backend = AsyncHttpBackend::new(
    ///     "https://datasets.example.com/snapshot.st".to_string(),
    ///     false
    /// )?;
    ///
    /// // Localhost (blocked by default)
    /// let local = AsyncHttpBackend::new(
    ///     "http://127.0.0.1:8000/snapshot.st".to_string(),
    ///     true // explicitly allow restricted IPs
    /// );
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    pub fn new(url: String, allow_restricted: bool) -> Result<Self> {
        let safe_url = validate_url(&url, allow_restricted)?;

        let runtime = Runtime::new().map_err(|e| StrataError::Io(Error::other(e)))?;

        let client = Client::builder()
            .build()
            .map_err(|e| StrataError::Io(Error::other(e)))?;

        let len = runtime.block_on(async {
            let resp = client
                .head(&safe_url)
                .send()
                .await
                .map_err(|e| StrataError::Io(Error::other(e)))?;

            if !resp.status().is_success() {
                return Err(StrataError::Io(Error::other(format!(
                    "HTTP error: {}",
                    resp.status()
                ))));
            }

            resp.headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|val| val.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or_else(|| {
                    StrataError::Io(Error::new(
                        ErrorKind::InvalidData,
                        "Missing Content-Length header",
                    ))
                })
        })?;

        Ok(Self {
            url: safe_url,
            client,
            len,
            runtime: Arc::new(runtime),
        })
    }
}

#[cfg(feature = "async-http")]
impl StorageBackend for AsyncHttpBackend {
    /// Reads exactly `len` bytes starting at `offset` using an async HTTP range request.
    ///
    /// This method sends an async `GET` request with a `Range: bytes=start-end` header
    /// to fetch only the requested data. The async operation is executed on the embedded
    /// Tokio runtime using `block_on()`, presenting a synchronous interface.
    ///
    /// # Parameters
    ///
    /// - `offset`: Starting byte offset from the beginning of the file
    /// - `len`: Number of bytes to read
    ///
    /// # Returns
    ///
    /// - `Ok(Bytes)`: A buffer containing exactly `len` bytes
    /// - `Err(StrataError::Io)`: If the network request fails or response is invalid
    ///
    /// # Errors
    ///
    /// Common error conditions:
    /// - **Network timeout**: Connection or read timeout (default reqwest timeout)
    /// - **HTTP error**: Server returns non-success status (not 200 or 206)
    /// - **Unexpected EOF**: Server returns fewer bytes than requested
    /// - **Connection failure**: Network unreachable, server down, DNS failure
    ///
    /// # Performance
    ///
    /// - **Latency**: 50-200ms per request (network RTT + server processing + runtime overhead)
    /// - **Throughput**: Limited by network bandwidth and server capabilities
    /// - **Connection reuse**: HTTP/1.1 Keep-Alive or HTTP/2 multiplexing via connection pool
    ///
    /// For best performance:
    /// - Use a block cache to minimize redundant requests
    /// - Prefer larger reads (16KB-64KB) to amortize request overhead
    /// - Consider pre-warming the connection pool with an initial request
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[cfg(feature = "async-http")]
    /// # {
    /// use strata_core::store::http::AsyncHttpBackend;
    /// use strata_core::store::StorageBackend;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = AsyncHttpBackend::new(
    ///     "https://example.com/snapshot.st".to_string(),
    ///     false
    /// )?;
    ///
    /// // Read first 512 bytes
    /// let header = backend.read_exact(0, 512)?;
    /// assert_eq!(header.len(), 512);
    ///
    /// // Read 64KB block at offset 1MB
    /// let block = backend.read_exact(1024 * 1024, 65536)?;
    /// assert_eq!(block.len(), 65536);
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    fn read_exact(&self, offset: u64, len: usize) -> Result<Bytes> {
        let end = offset + len as u64 - 1;
        let range_header = format!("bytes={}-{}", offset, end);

        self.runtime.block_on(async {
            let resp = self
                .client
                .get(&self.url)
                .header("Range", range_header)
                .send()
                .await
                .map_err(|e| StrataError::Io(Error::other(e)))?;

            if !resp.status().is_success() {
                return Err(StrataError::Io(Error::other(format!(
                    "HTTP error: {}",
                    resp.status()
                ))));
            }

            let bytes = resp
                .bytes()
                .await
                .map_err(|e| StrataError::Io(Error::other(e)))?;

            if bytes.len() != len {
                return Err(StrataError::Io(Error::new(
                    ErrorKind::UnexpectedEof,
                    format!("Expected {} bytes, got {}", len, bytes.len()),
                )));
            }

            Ok(bytes)
        })
    }

    /// Returns the total file size in bytes.
    ///
    /// This value is obtained from the `Content-Length` header during the initial
    /// HEAD request and is cached for the lifetime of the backend.
    ///
    /// # Returns
    ///
    /// The file size in bytes as reported by the HTTP server.
    ///
    /// # Performance
    ///
    /// This method is a simple field access with no network I/O (O(1)).
    fn len(&self) -> u64 {
        self.len
    }
}
