//! Amazon S3 and S3-compatible object storage backends.
//!
//! This module provides `StorageBackend` implementations for accessing snapshots
//! stored in Amazon S3 or S3-compatible object storage systems (MinIO, DigitalOcean
//! Spaces, Wasabi, etc.). It uses range GET requests to fetch specific byte ranges
//! without downloading entire objects, enabling efficient random access to cloud-stored
//! snapshots.
//!
//! # Architecture
//!
//! The module provides two implementations:
//! - [`S3Backend`]: Synchronous/blocking implementation using `rust-s3` blocking client
//! - [`AsyncS3Backend`]: Asynchronous implementation using `rust-s3` async client + Tokio
//!
//! Both implementations use the AWS SDK v3 credential chain to authenticate:
//! 1. Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
//! 2. AWS credentials file (`~/.aws/credentials`)
//! 3. IAM instance profile (for EC2 instances)
//! 4. ECS task role (for containerized applications)
//!
//! # S3 Bucket and Key Format
//!
//! The backends require three pieces of information:
//! - **Bucket name**: The S3 bucket containing the snapshot (e.g., `"my-snapshots"`)
//! - **Object key**: The full path to the snapshot within the bucket (e.g., `"prod/2024/snapshot-001.st"`)
//! - **Region**: AWS region identifier (e.g., `"us-east-1"`, `"eu-west-1"`)
//!
//! For S3-compatible services (MinIO, etc.), you can provide a custom endpoint URL.
//!
//! # Thread Safety
//!
//! Both backends are fully thread-safe (`Send + Sync`):
//! - The underlying S3 client uses connection pooling and is designed for concurrent use
//! - Multiple threads can call `read_exact()` concurrently without coordination
//! - Each request is independent and does not affect others
//!
//! # Performance Characteristics
//!
//! - **Latency**: 50-200ms per request (depends on region, object size, and network)
//! - **Throughput**: Up to 100MB/s per connection (S3 scales with multiple connections)
//! - **Connection pooling**: Automatic via `rust-s3` client
//! - **Cost**: S3 charges per request (~$0.0004 per 1000 GET requests) and data transfer
//!
//! **Optimization strategies**:
//! - Use a block cache to minimize redundant S3 requests
//! - Prefer larger read sizes (64KB-1MB) to amortize request overhead
//! - Enable S3 Transfer Acceleration for cross-region access
//! - Use CloudFront CDN to cache frequently accessed snapshots
//!
//! # Credential Handling
//!
//! The backends use the standard AWS credential chain. Set credentials via:
//!
//! ## Environment Variables
//! ```bash
//! export AWS_ACCESS_KEY_ID="AKIAIOSFODNN7EXAMPLE"
//! export AWS_SECRET_ACCESS_KEY="wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
//! export AWS_REGION="us-east-1"
//! ```
//!
//! ## AWS Config File
//! ```ini
//! # ~/.aws/credentials
//! [default]
//! aws_access_key_id = AKIAIOSFODNN7EXAMPLE
//! aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
//!
//! # ~/.aws/config
//! [default]
//! region = us-east-1
//! ```
//!
//! ## IAM Instance Profile (EC2)
//! When running on EC2, credentials are automatically fetched from the instance
//! metadata service. Ensure the instance has an IAM role with S3 read permissions.
//!
//! # Region Selection
//!
//! The region parameter accepts standard AWS region identifiers:
//! - `us-east-1` (US East - Virginia)
//! - `us-west-2` (US West - Oregon)
//! - `eu-west-1` (Europe - Ireland)
//! - `ap-southeast-1` (Asia Pacific - Singapore)
//! - See: https://docs.aws.amazon.com/general/latest/gr/rande.html
//!
//! For S3-compatible services, use a custom endpoint:
//! ```no_run
//! # #[cfg(feature = "s3")]
//! # {
//! use strata_core::store::s3::AsyncS3Backend;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = AsyncS3Backend::new(
//!     "my-bucket".to_string(),
//!     "snapshots/data.st".to_string(),
//!     "us-east-1".to_string(),
//!     Some("https://minio.example.com".to_string()) // Custom endpoint
//! )?;
//! # Ok(())
//! # }
//! # }
//! ```
//!
//! # When to Use This Backend
//!
//! Prefer S3 backends when:
//! - Snapshots are stored in S3 or S3-compatible object storage
//! - Building cloud-native applications that need durable, scalable storage
//! - Implementing disaster recovery with cross-region replication
//! - Distributing snapshots globally via S3 Transfer Acceleration
//!
//! Avoid S3 backends when:
//! - Latency is critical (prefer local or HTTP with CDN)
//! - Cost is a concern (S3 requests and data transfer are metered)
//! - Network connectivity is unreliable
//!
//! # Error Handling
//!
//! Common error conditions:
//! - **Credentials missing** (`PermissionDenied`): No valid AWS credentials found
//! - **Access denied** (`403 Forbidden`): IAM policy does not allow S3 access
//! - **Object not found** (`404 Not Found`): Bucket or key does not exist
//! - **Invalid region**: Region string is not recognized
//! - **Network timeout**: Request timeout or connection failure
//!
//! The backends do **not** implement automatic retries. Wrap backends with retry
//! logic for production use.
//!
//! # Examples
//!
//! ## Synchronous Backend
//!
//! ```no_run
//! use strata_core::store::s3::S3Backend;
//! use strata_core::store::StorageBackend;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Open a snapshot from S3
//! let backend = S3Backend::new(
//!     "my-snapshots".to_string(),     // Bucket name
//!     "prod/snapshot-001.st".to_string(), // Object key
//!     "us-east-1".to_string()         // Region
//! )?;
//!
//! println!("Snapshot size: {} bytes", backend.len());
//!
//! // Read first 512 bytes
//! let header = backend.read_exact(0, 512)?;
//! assert_eq!(header.len(), 512);
//! # Ok(())
//! # }
//! ```
//!
//! ## Asynchronous Backend (requires `s3` feature)
//!
//! ```no_run
//! # #[cfg(feature = "s3")]
//! # {
//! use strata_core::store::s3::AsyncS3Backend;
//! use strata_core::store::StorageBackend;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = AsyncS3Backend::new(
//!     "my-snapshots".to_string(),
//!     "prod/snapshot-001.st".to_string(),
//!     "us-east-1".to_string(),
//!     None // Use AWS endpoint
//! )?;
//!
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
//! use strata_core::store::s3::S3Backend;
//! use strata_core::store::StorageBackend;
//! use std::sync::Arc;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = Arc::new(S3Backend::new(
//!     "my-snapshots".to_string(),
//!     "data.st".to_string(),
//!     "us-east-1".to_string()
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

/// Blocking S3 storage backend.
pub mod sync;

/// Asynchronous S3 storage backend (feature-gated).
#[cfg(feature = "s3")]
pub mod async_client;

pub use sync::S3Backend;

#[cfg(feature = "s3")]
pub use async_client::AsyncS3Backend;
