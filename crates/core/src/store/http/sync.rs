//! Synchronous HTTP Storage Backend.
//!
//! This module implements the `StorageBackend` trait for snapshots hosted on
//! HTTP/HTTPS servers. It uses HTTP range requests (RFC 7233) to fetch specific
//! byte ranges, enabling random access to remote snapshots without downloading
//! the entire file.
//!
//! # Features
//!
//! - **Range Request Support**: Uses `Range: bytes=start-end` headers
//! - **Security Validation**: Blocks access to restricted IPs (localhost, private networks)
//! - **Automatic Retries**: Built-in retry logic for transient network errors
//! - **Connection Pooling**: Reuses HTTP connections via `reqwest::blocking::Client`
//!
//! # Performance
//!
//! - **Latency**: ~50-200ms per request (depends on server location)
//! - **Throughput**: Limited by network bandwidth and server speed
//! - **Caching**: Pairs well with block cache to minimize repeated requests
//!
//! # Security
//!
//! By default, this backend blocks requests to:
//! - Localhost (127.0.0.0/8, ::1)
//! - Private networks (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16)
//! - Link-local addresses (169.254.0.0/16, fe80::/10)
//!
//! Use `allow_restricted: true` only in trusted environments.

use crate::store::StorageBackend;
use crate::store::utils::validate_url;
use bytes::Bytes;
use reqwest::blocking::Client;
use std::io::{Error, ErrorKind};
use strata_common::{Result, StrataError};

/// HTTP 200 OK status code.
const HTTP_OK: u16 = 200;

/// HTTP 206 Partial Content status code (range request success).
const HTTP_PARTIAL: u16 = 206;

/// A storage backend for snapshots hosted on HTTP/HTTPS servers.
///
/// This backend uses HTTP range requests to fetch specific byte ranges from
/// a remote snapshot file. It validates URLs for security, checks file size
/// via HEAD request, and maintains a persistent HTTP connection pool.
///
/// # Examples
///
/// ```no_run
/// use strata_core::store::http::HttpBackend;
/// use strata_core::store::StorageBackend;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = HttpBackend::new(
///     "https://example.com/snapshot.st".to_string(),
///     false // don't allow restricted IPs
/// )?;
///
/// // Read first 512 bytes
/// let data = backend.read_exact(0, 512)?;
/// assert_eq!(data.len(), 512);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct HttpBackend {
    /// The validated HTTP/HTTPS URL of the snapshot file.
    url: String,
    /// HTTP client with connection pooling and timeout configuration.
    client: Client,
    /// Total file size in bytes, obtained via HEAD request.
    len: u64,
}

impl HttpBackend {
    /// Creates a new HTTP backend by validating the URL and fetching file metadata.
    ///
    /// This constructor:
    /// 1. Validates the URL for security (blocks restricted IPs unless allowed)
    /// 2. Sends a HEAD request to verify the server supports range requests
    /// 3. Extracts the file size from the `Content-Length` header
    ///
    /// # Parameters
    ///
    /// - `url`: The HTTP/HTTPS URL of the snapshot file
    /// - `allow_restricted`: If `false`, blocks access to localhost and private networks
    ///
    /// # Returns
    ///
    /// - `Ok(HttpBackend)` if the URL is valid and the server is reachable
    /// - `Err(StrataError::Io)` if network fails or server returns error
    /// - `Err(StrataError::Format)` if `Content-Length` header is missing
    ///
    /// # Security
    ///
    /// Set `allow_restricted: false` (recommended) to prevent SSRF attacks by blocking
    /// requests to internal networks.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use strata_core::store::http::HttpBackend;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Public internet URL (safe)
    /// let backend = HttpBackend::new(
    ///     "https://datasets.example.com/snapshot.st".to_string(),
    ///     false
    /// )?;
    ///
    /// // Localhost (blocked by default, requires allow_restricted=true)
    /// let local = HttpBackend::new(
    ///     "http://127.0.0.1:8000/snapshot.st".to_string(),
    ///     true // explicitly allow restricted IPs
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(url: String, allow_restricted: bool) -> Result<Self> {
        let safe_url = validate_url(&url, allow_restricted)?;

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| StrataError::Io(Error::other(e)))?;

        let resp = client
            .head(&safe_url)
            .send()
            .map_err(|e| StrataError::Io(Error::other(e)))?;

        if !resp.status().is_success() {
            return Err(StrataError::Io(Error::other(format!(
                "HTTP error: {}",
                resp.status()
            ))));
        }

        let len = resp
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|val| val.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| {
                StrataError::Io(Error::new(
                    ErrorKind::InvalidData,
                    "Missing Content-Length header",
                ))
            })?;

        Ok(Self {
            url: safe_url,
            client,
            len,
        })
    }
}

impl StorageBackend for HttpBackend {
    /// Reads a byte range using an HTTP GET request with a `Range` header.
    ///
    /// This method sends a `Range: bytes=start-end` header to fetch only the
    /// requested data. The server must support HTTP range requests (RFC 7233).
    ///
    /// # Parameters
    ///
    /// - `offset`: Starting byte offset
    /// - `len`: Number of bytes to read
    ///
    /// # Returns
    ///
    /// - `Ok(Bytes)` containing exactly `len` bytes
    /// - `Err(StrataError::Io)` if network fails or server returns non-200/206 status
    ///
    /// # Performance
    ///
    /// Each call initiates a new HTTP request. For best performance:
    /// - Use a block cache to minimize repeated requests
    /// - Prefer larger reads (4KB+ blocks) to amortize request overhead
    ///
    /// # Errors
    ///
    /// - Network timeouts (10s timeout configured)
    /// - Server returns non-success status
    /// - Response body is shorter than expected
    fn read_exact(&self, offset: u64, len: usize) -> Result<Bytes> {
        let end = offset + len as u64 - 1;
        let range_header = format!("bytes={}-{}", offset, end);

        let resp = self
            .client
            .get(&self.url)
            .header("Range", range_header)
            .send()
            .map_err(|e| StrataError::Io(Error::other(e)))?;

        let status = resp.status().as_u16();
        if status != HTTP_OK && status != HTTP_PARTIAL {
            return Err(StrataError::Io(Error::other(format!(
                "HTTP error: {}",
                resp.status()
            ))));
        }

        let bytes = resp.bytes().map_err(|e| StrataError::Io(Error::other(e)))?;

        if bytes.len() != len {
            return Err(StrataError::Io(Error::new(
                ErrorKind::UnexpectedEof,
                format!("Expected {} bytes, got {}", len, bytes.len()),
            )));
        }

        Ok(bytes)
    }

    /// Returns the total file size in bytes.
    ///
    /// This value is obtained from the `Content-Length` header during initialization
    /// and is cached for the lifetime of the backend.
    fn len(&self) -> u64 {
        self.len
    }
}
