//! HTTP storage backend with embedded Tokio runtime.
//!
//! This module provides an HTTP storage backend that wraps the `reqwest` async
//! client in an embedded Tokio runtime. This allows the backend to present a
//! synchronous `StorageBackend` interface while leveraging async I/O internally
//! for efficient concurrent operations.
//!
//! # Architecture
//!
//! The [`HttpBackend`] embeds a Tokio runtime (`Arc<Runtime>`) and uses
//! `runtime.block_on()` to execute async operations synchronously. This design:
//! - Maintains compatibility with the synchronous `StorageBackend` trait
//! - Enables efficient connection pooling and concurrent requests via `reqwest`
//! - Provides async benefits (low memory overhead per connection) without requiring
//!   callers to use async/await
//!
//! # Thread Safety
//!
//! The backend is fully thread-safe (`Send + Sync`):
//! - The `reqwest::Client` is designed for concurrent use
//! - The `Arc<Runtime>` is shared safely across threads
//! - Multiple threads can call `read_exact()` concurrently without coordination
//!
//! # Security
//!
//! This backend validates URLs to prevent SSRF attacks:
//! - Blocks access to localhost and private networks by default
//! - Set `allow_restricted: true` only in trusted environments
//! - See [`validate_url`](crate::store::utils::validate_url) for details
//!
//! # Examples
//!
//! ```no_run
//! use strata_core::store::http::HttpBackend;
//! use strata_core::store::StorageBackend;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = HttpBackend::new(
//!     "https://cdn.example.com/snapshots/data.st".to_string(),
//!     false // block restricted IPs
//! )?;
//!
//! println!("Snapshot size: {} bytes", backend.len());
//!
//! let header = backend.read_exact(0, 512)?;
//! assert_eq!(header.len(), 512);
//! # Ok(())
//! # }
//! ```

use crate::store::StorageBackend;
use crate::store::utils::validate_url;
use bytes::Bytes;
use reqwest::Client;
use std::io::{Error, ErrorKind};
use std::sync::Arc;
use strata_common::{Result, StrataError};
use tokio::runtime::Runtime;

/// HTTP storage backend with embedded Tokio runtime.
///
/// This backend wraps an async `reqwest::Client` and Tokio `Runtime` to provide
/// synchronous `StorageBackend` operations while leveraging async I/O internally.
/// It validates URLs for security, maintains a connection pool, and performs
/// range requests to fetch specific byte ranges.
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
///     false
/// )?;
///
/// let data = backend.read_exact(8192, 4096)?;
/// assert_eq!(data.len(), 4096);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct HttpBackend {
    url: String,
    client: Client,
    len: u64,
    runtime: Arc<Runtime>,
}

impl HttpBackend {
    /// Creates a new HTTP backend by validating the URL and fetching file metadata.
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

impl StorageBackend for HttpBackend {
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

    fn len(&self) -> u64 {
        self.len
    }
}
