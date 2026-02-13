//! Amazon S3 and S3-compatible object storage backend.
//!
//! This module provides a `StorageBackend` implementation for accessing snapshots
//! stored in Amazon S3 or S3-compatible object storage systems (MinIO, DigitalOcean
//! Spaces, Wasabi, etc.). It uses range GET requests to fetch specific byte ranges
//! without downloading entire objects, enabling efficient random access to cloud-stored
//! snapshots.
//!
//! # Architecture
//!
//! The [`S3Backend`] wraps the `rust-s3` async client in an embedded Tokio runtime,
//! presenting a synchronous `StorageBackend` interface while leveraging async I/O
//! internally for efficient concurrent operations. It supports both AWS S3 and
//! S3-compatible storage via custom endpoints.
//!
//! The backend uses the AWS SDK v3 credential chain to authenticate:
//! 1. Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
//! 2. AWS credentials file (`~/.aws/credentials`)
//! 3. IAM instance profile (for EC2 instances)
//! 4. ECS task role (for containerized applications)
//!
//! # Thread Safety
//!
//! The backend is fully thread-safe (`Send + Sync`):
//! - The underlying S3 client uses connection pooling and is designed for concurrent use
//! - Multiple threads can call `read_exact()` concurrently without coordination
//! - Each request is independent and does not affect others
//!
//! # Examples
//!
//! ```no_run
//! # #[cfg(feature = "s3")]
//! # {
//! use strata_core::store::s3::S3Backend;
//! use strata_core::store::StorageBackend;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = S3Backend::new(
//!     "my-snapshots".to_string(),
//!     "prod/snapshot-001.st".to_string(),
//!     "us-east-1".to_string(),
//!     None // Use AWS endpoint
//! )?;
//!
//! println!("Snapshot size: {} bytes", backend.len());
//!
//! let data = backend.read_exact(0, 512)?;
//! assert_eq!(data.len(), 512);
//! # Ok(())
//! # }
//! # }
//! ```

/// S3 storage backend (feature-gated).
#[cfg(feature = "s3")]
pub mod async_client;

#[cfg(feature = "s3")]
pub use async_client::S3Backend;
