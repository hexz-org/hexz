//! S3-compatible object storage backend (feature-gated).

#[cfg(feature = "s3")]
pub mod async_client;

#[cfg(feature = "s3")]
pub use async_client::S3Backend;
