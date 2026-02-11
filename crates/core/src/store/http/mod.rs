//! HTTP/HTTPS storage backends for remote snapshot access.
//!
//! This module provides `StorageBackend` implementations that fetch snapshot data
//! from HTTP/HTTPS servers using RFC 7233 range requests. It enables accessing
//! remote snapshots without downloading entire files, making it ideal for cloud-native
//! deployments, CI/CD pipelines, and distributed systems.
//!
//! # Architecture
//!
//! The module provides two implementations:
//! - [`HttpBackend`]: Synchronous/blocking implementation using `reqwest::blocking`
//! - [`AsyncHttpBackend`]: Asynchronous implementation using `reqwest` + Tokio (feature-gated)
//!
//! Both implementations use HTTP range requests (`Range: bytes=start-end`) to fetch
//! only the requested byte ranges, avoiding unnecessary data transfer. The backends
//! maintain persistent HTTP connection pools for optimal performance.
//!
//! # Security
//!
//! To prevent SSRF (Server-Side Request Forgery) attacks, these backends implement
//! URL validation that blocks access to:
//! - **Loopback addresses**: 127.0.0.0/8, ::1
//! - **Private networks**: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
//! - **Link-local addresses**: 169.254.0.0/16 (including AWS metadata at 169.254.169.254)
//! - **IPv6 private ranges**: fc00::/7, fe80::/10
//!
//! Use `allow_restricted: true` only in trusted environments (e.g., local development,
//! trusted internal networks).
//!
//! # Thread Safety
//!
//! Both backends are fully thread-safe (`Send + Sync`):
//! - The underlying `reqwest::Client` uses connection pooling and is designed for sharing
//! - Multiple threads can call `read_exact()` concurrently without coordination
//! - Each request is independent and does not affect others
//!
//! # Performance Characteristics
//!
//! - **Latency**: 50-200ms per request (depends on network RTT and server location)
//! - **Throughput**: Limited by network bandwidth and server capabilities
//! - **Connection pooling**: Reuses TCP connections to minimize handshake overhead
//! - **Timeout**: 10 seconds per request (configurable in sync backend)
//!
//! **Optimization strategies**:
//! - Use a block cache to minimize redundant network requests
//! - Prefer larger read sizes (16KB-64KB) to amortize request overhead
//! - Deploy snapshots to geographically distributed CDNs for low latency
//!
//! # When to Use This Backend
//!
//! Prefer HTTP backends when:
//! - Snapshots are hosted on cloud storage with HTTP endpoints (e.g., S3 pre-signed URLs)
//! - Building stateless services that don't want to manage local storage
//! - Implementing CI/CD pipelines that need ephemeral snapshot access
//! - Distributing snapshots via CDN for global low-latency access
//!
//! Avoid HTTP backends when:
//! - Network latency is critical (prefer local backends)
//! - Snapshots are accessed very frequently (cache locally instead)
//! - Bandwidth costs are prohibitive
//!
//! # Error Handling and Retry Strategies
//!
//! The backends do **not** implement automatic retries. Callers should wrap backends
//! with retry logic for production use. Common transient errors:
//! - **Connection timeout**: Network congestion or server overload
//! - **502/503 errors**: Backend server temporarily unavailable
//! - **DNS resolution failure**: Transient DNS issues
//!
//! Example retry strategy (not implemented in this module):
//! ```text
//! 1. Initial request fails
//! 2. Wait 100ms (exponential backoff: 100ms, 200ms, 400ms, ...)
//! 3. Retry up to 3 times
//! 4. If all retries fail, propagate error to caller
//! ```
//!
//! # Range Request Requirements
//!
//! The HTTP server must support:
//! - **Content-Length header**: Required to determine file size
//! - **Range requests** (RFC 7233): Server should return `206 Partial Content`
//! - **Accept-Ranges: bytes**: Indicates server supports byte ranges
//!
//! Most modern HTTP servers and object storage systems support these features.
//!
//! # Examples
//!
//! ## Synchronous Backend
//!
//! ```no_run
//! use strata_core::store::http::HttpBackend;
//! use strata_core::store::StorageBackend;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Open a remote snapshot
//! let backend = HttpBackend::new(
//!     "https://cdn.example.com/snapshots/data.st".to_string(),
//!     false // block restricted IPs
//! )?;
//!
//! // Read first 512 bytes (header)
//! let header = backend.read_exact(0, 512)?;
//! assert_eq!(header.len(), 512);
//!
//! // Read 4KB block at offset 1MB
//! let block = backend.read_exact(1024 * 1024, 4096)?;
//! assert_eq!(block.len(), 4096);
//! # Ok(())
//! # }
//! ```
//!
//! ## Asynchronous Backend (requires `async-http` feature)
//!
//! ```no_run
//! # #[cfg(feature = "async-http")]
//! # {
//! use strata_core::store::http::AsyncHttpBackend;
//! use strata_core::store::StorageBackend;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = AsyncHttpBackend::new(
//!     "https://cdn.example.com/snapshots/data.st".to_string(),
//!     false
//! )?;
//!
//! // Reads use internal Tokio runtime
//! let data = backend.read_exact(0, 512)?;
//! assert_eq!(data.len(), 512);
//! # Ok(())
//! # }
//! # }
//! ```
//!
//! ## Concurrent Access
//!
//! ```no_run
//! use strata_core::store::http::HttpBackend;
//! use strata_core::store::StorageBackend;
//! use std::sync::Arc;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = Arc::new(HttpBackend::new(
//!     "https://cdn.example.com/data.st".to_string(),
//!     false
//! )?);
//!
//! // Spawn multiple threads reading concurrently
//! let handles: Vec<_> = (0..4)
//!     .map(|i| {
//!         let b = backend.clone();
//!         std::thread::spawn(move || {
//!             b.read_exact(i * 1024, 1024)
//!         })
//!     })
//!     .collect();
//!
//! for handle in handles {
//!     let result = handle.join().unwrap()?;
//!     assert_eq!(result.len(), 1024);
//! }
//! # Ok(())
//! # }
//! ```

/// Blocking HTTP storage backend using range requests.
pub mod sync;

/// Asynchronous HTTP storage backend (feature-gated).
///
/// When the `async-http` feature is enabled, this module provides a non-blocking
/// HTTP backend suitable for async runtimes like Tokio.
#[cfg(feature = "async-http")]
pub mod async_client;

pub use sync::HttpBackend;

#[cfg(feature = "async-http")]
pub use async_client::AsyncHttpBackend;
