/// Blocking S3 storage backend.
pub mod sync;

/// Asynchronous S3 storage backend (feature-gated).
#[cfg(feature = "s3")]
pub mod async_client;

pub use sync::S3Backend;

#[cfg(feature = "s3")]
pub use async_client::AsyncS3Backend;
