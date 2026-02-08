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
