//! HTTP/HTTPS storage backend for remote snapshot access.
//!
//! This module provides a `StorageBackend` implementation that fetches snapshot data
//! from HTTP/HTTPS servers using RFC 7233 range requests. It enables accessing
//! remote snapshots without downloading entire files, making it ideal for cloud-native
//! deployments, CI/CD pipelines, and distributed systems.
//!
//! # Architecture
//!
//! The [`HttpBackend`] wraps the `reqwest` async client in an embedded Tokio runtime,
//! presenting a synchronous `StorageBackend` interface while leveraging async I/O
//! internally for efficient concurrent operations.
//!
//! It uses HTTP range requests (`Range: bytes=start-end`) to fetch only the
//! requested byte ranges, avoiding unnecessary data transfer. The backend maintains
//! persistent HTTP connection pools for optimal performance.
//!
//! # Security
//!
//! To prevent SSRF (Server-Side Request Forgery) attacks, the backend implements
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
//! The backend is fully thread-safe (`Send + Sync`):
//! - The underlying `reqwest::Client` uses connection pooling and is designed for sharing
//! - Multiple threads can call `read_exact()` concurrently without coordination
//! - Each request is independent and does not affect others
//!
//! # Examples
//!
//! ```no_run
//! use hexz_core::store::http::HttpBackend;
//! use hexz_core::store::StorageBackend;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = HttpBackend::new(
//!     "https://cdn.example.com/snapshots/data.hxz".to_string(),
//!     false // block restricted IPs
//! )?;
//!
//! let header = backend.read_exact(0, 512)?;
//! assert_eq!(header.len(), 512);
//! # Ok(())
//! # }
//! ```

/// HTTP storage backend using async reqwest + Tokio.
pub mod async_client;

pub use async_client::HttpBackend;
