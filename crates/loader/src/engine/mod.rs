//! Pure Rust engine abstraction layer for Strata snapshot access.
//!
//! This module provides a storage-backend-agnostic interface for opening, configuring,
//! and reading Strata snapshot files. It serves as a dependency-free foundation that
//! can be consumed by Python bindings (via PyO3), standalone Rust applications, or
//! other language FFI layers without requiring Python runtime dependencies.
//!
//! # Architecture Overview
//!
//! The engine layer separates snapshot access logic from language-specific bindings:
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │   Python / Other Language Bindings  │
//! └──────────────┬──────────────────────┘
//!                │
//!                v
//! ┌─────────────────────────────────────┐
//! │      Engine Layer (This Module)     │  ← Pure Rust, no PyO3
//! │  - Backend selection & config       │
//! │  - Iterator abstractions            │
//! │  - Shuffle/sampling logic           │
//! └──────────────┬──────────────────────┘
//!                │
//!                v
//! ┌─────────────────────────────────────┐
//! │        strata-core::StrataFile      │  ← Core I/O + compression
//! └──────────────┬──────────────────────┘
//!                │
//!                v
//! ┌─────────────────────────────────────┐
//! │       Storage Backends              │
//! │  - Local filesystem                 │
//! │  - HTTP/HTTPS                       │
//! │  - S3 / S3-compatible               │
//! └─────────────────────────────────────┘
//! ```
//!
//! # Backend Selection
//!
//! The engine automatically selects the appropriate storage backend based on the
//! URI scheme provided in [`OpenConfig::path`]:
//!
//! | Scheme           | Backend         | Use Case                              |
//! |------------------|-----------------|---------------------------------------|
//! | `/path/to/file`  | FileBackend     | Local disk access                     |
//! | `http://...`     | HttpBackend     | Remote HTTP-accessible snapshots      |
//! | `https://...`    | HttpBackend     | TLS-secured remote snapshots          |
//! | `s3://bucket/key`| AsyncS3Backend  | AWS S3 or S3-compatible object stores |
//!
//! ## Security Considerations
//!
//! The [`OpenConfig::allow_restricted`] flag controls access to private IP ranges
//! (RFC 1918: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`) when using HTTP or
//! S3 backends. This mitigates Server-Side Request Forgery (SSRF) attacks in hosted
//! environments where user-supplied URIs may be passed to the loader.
//!
//! # Data Loading Pipeline
//!
//! A typical ML data loading pipeline uses this module as follows:
//!
//! 1. **Open Snapshot**: Call [`open_snapshot`] with an [`OpenConfig`] to obtain
//!    an `Arc<StrataFile>` handle.
//! 2. **Configure Iteration**: Use [`iterator::IterConfig`] to set block size,
//!    prefetch count, and target stream (Disk vs. Memory).
//! 3. **Shuffle (Optional)**: Generate randomized indices via [`shuffle::shuffled_indices`]
//!    for non-sequential access patterns.
//! 4. **Iterate**: Consume data using [`iterator::SnapshotIterator`] or direct
//!    [`read_stream`] calls.
//!
//! ## Example: Sequential Reading
//!
//! ```rust,no_run
//! use strata_loader::engine::{OpenConfig, open_snapshot, iterator::{IterConfig, SnapshotIterator}};
//! use strata_core::api::stratafile::SnapshotStream;
//!
//! let config = OpenConfig {
//!     path: "/data/snapshot.st".to_string(),
//!     s3_region: None,
//!     endpoint_url: None,
//!     allow_restricted: false,
//!     prefetch_count: 4,
//!     cache_capacity_bytes: Some(8 * 1024 * 1024), // 8MB cache
//! };
//!
//! let snap = open_snapshot(config).expect("Failed to open snapshot");
//!
//! let iter_config = IterConfig {
//!     block_size: 65536,
//!     prefetch_count: 4,
//!     stream: SnapshotStream::Disk,
//! };
//!
//! let mut iter = SnapshotIterator::new(snap, iter_config);
//! for block in iter {
//!     let data = block.expect("Read error");
//!     // Process data...
//! }
//! ```
//!
//! ## Example: Shuffled Random Access
//!
//! ```rust,no_run
//! use strata_loader::engine::{OpenConfig, open_snapshot, shuffle::shuffled_indices, stream_size};
//! use strata_core::api::stratafile::SnapshotStream;
//!
//! let config = OpenConfig {
//!     path: "s3://my-bucket/snapshot.st".to_string(),
//!     s3_region: Some("us-west-2".to_string()),
//!     endpoint_url: None,
//!     allow_restricted: false,
//!     prefetch_count: 8,
//!     cache_capacity_bytes: Some(16 * 1024 * 1024),
//! };
//!
//! let snap = open_snapshot(config).expect("Failed to open S3 snapshot");
//! let size = stream_size(&snap, SnapshotStream::Disk);
//!
//! // Generate shuffled block indices for randomized training
//! let sample_size = 4096;
//! let num_samples = (size / sample_size as u64) as usize;
//! let indices = shuffled_indices(num_samples, 42);
//!
//! for idx in indices.iter().take(100) {
//!     let offset = (*idx as u64) * sample_size;
//!     let data = strata_loader::engine::read_stream(&snap, SnapshotStream::Disk, offset, sample_size as usize)
//!         .expect("Read failed");
//!     // Process shuffled sample...
//! }
//! ```
//!
//! # Performance Characteristics
//!
//! - **Prefetching**: The `prefetch_count` parameter enables automatic background
//!   fetching of subsequent blocks. This is critical for network backends (HTTP, S3)
//!   where latency dominates. Typical values: 4-16 for sequential workloads.
//!
//! - **Caching**: The `cache_capacity_bytes` controls the decompressed block cache.
//!   Larger caches reduce repeated decompression for random access patterns but
//!   increase memory usage. Default: ~4MB (effective for 4KB block size).
//!
//! - **Zero-Copy**: Data returned by [`read_stream`] is allocated in Rust and can
//!   be transferred to Python via the buffer protocol without intermediate copies
//!   (see [`crate::tensor::numpy`] for details).
//!
//! # Submodules
//!
//! - [`iterator`]: Sequential and configurable iteration over snapshot streams.
//! - [`shuffle`]: Deterministic Fisher-Yates index shuffling for ML data randomization.
//!
//! # Thread Safety
//!
//! All functions in this module are thread-safe. The underlying `StrataFile` is
//! `Send + Sync`, and the engine layer uses `Arc` to enable shared ownership across
//! threads. This allows parallel data loading with per-thread iterators.

pub mod iterator;
pub mod shuffle;

use std::sync::Arc;
use strata_common::constants::DEFAULT_ZSTD_LEVEL;
use strata_core::StrataFile;
use strata_core::algo::compression::{Compressor, lz4::Lz4Compressor, zstd::ZstdCompressor};
use strata_core::api::stratafile::SnapshotStream;
use strata_core::format::header::{CompressionType, StrataHeader};
use strata_core::format::magic::HEADER_SIZE;
use strata_core::store::StorageBackend;

/// Errors that can occur when opening or reading snapshots.
///
/// This enum encapsulates all failure modes that can occur during snapshot
/// initialization and access. Errors are designed to be user-facing and include
/// sufficient context for debugging.
#[derive(Debug)]
pub enum OpenError {
    /// The URI scheme is not recognized by any available backend.
    ///
    /// Currently supported schemes: local paths, `http://`, `https://`, `s3://`.
    /// If you see this error, verify the URI format matches one of these patterns.
    UnsupportedScheme(String),

    /// I/O error occurred during backend initialization or snapshot access.
    ///
    /// Common causes:
    /// - File does not exist (local backend)
    /// - Network timeout or DNS resolution failure (HTTP/S3)
    /// - Insufficient permissions (local or S3)
    /// - Disk full or read-only filesystem
    Io(String),

    /// The snapshot header is malformed, corrupt, or from an unsupported version.
    ///
    /// This typically indicates:
    /// - The file is not a valid Strata snapshot
    /// - The snapshot was created by an incompatible version of Strata
    /// - Data corruption occurred during storage or transfer
    InvalidHeader(String),

    /// S3 URI does not match the expected `s3://bucket/key` format.
    ///
    /// Valid S3 URIs must include both a bucket name and object key:
    /// - ✓ `s3://my-bucket/path/to/snapshot.st`
    /// - ✗ `s3://my-bucket` (missing key)
    /// - ✗ `s3:///path` (missing bucket)
    InvalidS3Uri(String),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::UnsupportedScheme(s) => write!(f, "Unsupported scheme: {}", s),
            OpenError::Io(s) => write!(f, "I/O error: {}", s),
            OpenError::InvalidHeader(s) => write!(f, "Invalid header: {}", s),
            OpenError::InvalidS3Uri(s) => write!(f, "Invalid S3 URI: {}", s),
        }
    }
}

impl std::error::Error for OpenError {}

/// Configuration for opening a Strata snapshot from any supported backend.
///
/// This struct aggregates all parameters required to locate, authenticate, and
/// optimize access to a snapshot, whether it resides on a local disk, remote
/// HTTP server, or cloud object storage.
///
/// # Backend Selection
///
/// The `path` field determines which storage backend is used:
///
/// - **Local filesystem**: Absolute or relative paths without a scheme.
/// - **HTTP/HTTPS**: URLs starting with `http://` or `https://`.
/// - **S3**: URIs starting with `s3://bucket-name/key`.
///
/// # Performance Tuning
///
/// The `prefetch_count` and `cache_capacity_bytes` parameters control memory vs.
/// throughput tradeoffs:
///
/// | Workload Type       | prefetch_count | cache_capacity_bytes |
/// |---------------------|----------------|----------------------|
/// | Sequential streaming| 8-16           | 4MB-16MB             |
/// | Random access       | 0-4            | 16MB-64MB            |
/// | Low-latency network | 16-32          | 32MB+                |
/// | Memory-constrained  | 2-4            | 2MB-4MB              |
///
/// # Security
///
/// Set `allow_restricted = false` (default) in multi-tenant or web-facing
/// environments to prevent SSRF attacks via user-supplied URIs.
///
/// # Examples
///
/// ## Local File with Default Settings
///
/// ```rust
/// use strata_loader::engine::OpenConfig;
///
/// let config = OpenConfig {
///     path: "/mnt/data/snapshot.st".to_string(),
///     s3_region: None,
///     endpoint_url: None,
///     allow_restricted: false,
///     prefetch_count: 0,
///     cache_capacity_bytes: None,
/// };
/// ```
///
/// ## S3 with Prefetching and Large Cache
///
/// ```rust
/// use strata_loader::engine::OpenConfig;
///
/// let config = OpenConfig {
///     path: "s3://ml-datasets/imagenet-2024/snapshot.st".to_string(),
///     s3_region: Some("eu-west-1".to_string()),
///     endpoint_url: None,
///     allow_restricted: false,
///     prefetch_count: 16,  // aggressive prefetch for high-throughput training
///     cache_capacity_bytes: Some(64 * 1024 * 1024),  // 64MB cache
/// };
/// ```
///
/// ## MinIO (S3-Compatible) with Custom Endpoint
///
/// ```rust
/// use strata_loader::engine::OpenConfig;
///
/// let config = OpenConfig {
///     path: "s3://my-bucket/data.st".to_string(),
///     s3_region: Some("us-east-1".to_string()),  // MinIO ignores region but field is required
///     endpoint_url: Some("https://minio.internal.company.com".to_string()),
///     allow_restricted: true,  // internal network access required
///     prefetch_count: 4,
///     cache_capacity_bytes: Some(8 * 1024 * 1024),
/// };
/// ```
pub struct OpenConfig {
    /// Path or URI to the snapshot file.
    ///
    /// Supported formats:
    /// - **Local**: `/path/to/snap.st` or `./relative/path.st`
    /// - **HTTP**: `http://example.com/snap.st`
    /// - **HTTPS**: `https://cdn.example.com/datasets/snap.st`
    /// - **S3**: `s3://bucket-name/prefix/snap.st`
    ///
    /// The scheme determines which backend handles I/O operations.
    pub path: String,

    /// AWS Region for S3 requests.
    ///
    /// Required for S3 URIs. Common values: `us-east-1`, `eu-west-1`, `ap-southeast-1`.
    /// If `None` and using S3, defaults to `us-east-1`.
    ///
    /// Ignored for non-S3 backends.
    pub s3_region: Option<String>,

    /// Custom endpoint URL for S3-compatible storage systems.
    ///
    /// Use this to connect to non-AWS S3 implementations like:
    /// - MinIO: `http://localhost:9000`
    /// - DigitalOcean Spaces: `https://nyc3.digitaloceanspaces.com`
    /// - Ceph RADOS Gateway: `https://ceph.internal.company.com`
    ///
    /// If `None`, uses the default AWS S3 endpoint. Ignored for non-S3 backends.
    pub endpoint_url: Option<String>,

    /// Allow connections to RFC 1918 private IP ranges.
    ///
    /// When `false` (default), HTTP and S3 backends reject connections to:
    /// - `10.0.0.0/8`
    /// - `172.16.0.0/12`
    /// - `192.168.0.0/16`
    /// - `127.0.0.0/8` (localhost)
    ///
    /// This prevents Server-Side Request Forgery (SSRF) attacks where an attacker
    /// supplies a URI pointing to internal services. Set to `true` only when:
    /// - Operating in a trusted environment (e.g., internal data pipelines)
    /// - Accessing snapshots on internal infrastructure (e.g., corporate MinIO)
    ///
    /// **Security Warning**: Enabling this in web-facing or multi-tenant systems
    /// may allow attackers to probe internal networks.
    pub allow_restricted: bool,

    /// Number of blocks to prefetch ahead during sequential reads.
    ///
    /// When > 0, the system automatically fetches the next `prefetch_count` blocks
    /// in the background while the current block is being processed. This overlaps
    /// network/disk I/O with computation, dramatically improving throughput for
    /// sequential workloads.
    ///
    /// **Performance Impact**:
    /// - **Local SSD**: Modest gains (10-20%), diminishing returns beyond 4
    /// - **HDD**: Significant gains (2-3x) due to reduced seek overhead
    /// - **Network (HTTP/S3)**: Critical (5-10x) due to latency hiding
    ///
    /// **Memory Cost**: Each prefetched block consumes memory equal to the
    /// compressed block size (~4KB-64KB typical). Total prefetch memory:
    /// `prefetch_count * avg_compressed_block_size`.
    ///
    /// Set to 0 to disable prefetching (useful for pure random access or
    /// memory-constrained environments).
    pub prefetch_count: u32,

    /// Decompressed block cache capacity in bytes.
    ///
    /// Controls the LRU cache size for decompressed blocks. Larger caches reduce
    /// repeated decompression for workloads with spatial locality but increase
    /// memory usage.
    ///
    /// **Tradeoffs**:
    /// - **Sequential access**: Small cache (4-8MB) sufficient, blocks rarely revisited
    /// - **Random access**: Large cache (32-128MB) beneficial, high reuse likelihood
    /// - **Shuffled ML training**: Medium cache (16-32MB), moderate reuse within epochs
    ///
    /// **Default Behavior**: If `None`, uses strata-core's default (~1024 blocks,
    /// ~4MB effective for 4KB block size).
    ///
    /// **Cache Eviction**: Least Recently Used (LRU) policy. When full, the oldest
    /// accessed block is evicted and must be re-read and decompressed on next access.
    pub cache_capacity_bytes: Option<usize>,
}

/// Opens a Strata snapshot from any supported backend.
///
/// This function is the primary entry point for accessing snapshot data. It performs
/// the following steps:
///
/// 1. **Backend Selection**: Inspects the URI scheme in `config.path` to determine
///    whether to use FileBackend (local), HttpBackend, or AsyncS3Backend.
/// 2. **Connection Establishment**: Initializes the selected backend, performing any
///    necessary authentication (S3) or DNS resolution (HTTP).
/// 3. **Header Validation**: Reads the snapshot header to extract compression type,
///    block size, and metadata.
/// 4. **Decompressor Initialization**: Instantiates the appropriate decompressor
///    (LZ4 or Zstd) based on the header.
/// 5. **Cache Configuration**: Sets up the block cache and prefetch pipeline using
///    the provided `prefetch_count` and `cache_capacity_bytes`.
///
/// The returned `Arc<StrataFile>` is thread-safe and can be cloned cheaply to share
/// across threads without duplicating the underlying cache or network connections.
///
/// # Parameters
///
/// - `config`: Snapshot location, backend credentials, and performance tuning parameters.
///
/// # Returns
///
/// - `Ok(Arc<StrataFile>)`: A reference-counted handle to the opened snapshot.
/// - `Err(OpenError)`: Failure during backend initialization, header parsing, or validation.
///
/// # Errors
///
/// - [`OpenError::UnsupportedScheme`]: The URI scheme is not recognized (e.g., `ftp://`).
/// - [`OpenError::Io`]: Backend creation failed (file not found, network unreachable, etc.).
/// - [`OpenError::InvalidHeader`]: Snapshot header is corrupt or from incompatible version.
/// - [`OpenError::InvalidS3Uri`]: S3 URI missing bucket or key component.
///
/// # Examples
///
/// ## Opening a Local Snapshot
///
/// ```rust,no_run
/// use strata_loader::engine::{OpenConfig, open_snapshot};
///
/// let config = OpenConfig {
///     path: "/data/mnist-train.st".to_string(),
///     s3_region: None,
///     endpoint_url: None,
///     allow_restricted: false,
///     prefetch_count: 0,
///     cache_capacity_bytes: None,
/// };
///
/// let snap = open_snapshot(config).expect("Failed to open snapshot");
/// println!("Snapshot opened successfully");
/// ```
///
/// ## Opening an S3 Snapshot with Prefetching
///
/// ```rust,no_run
/// use strata_loader::engine::{OpenConfig, open_snapshot};
///
/// let config = OpenConfig {
///     path: "s3://ml-datasets/coco-2024/train.st".to_string(),
///     s3_region: Some("us-west-2".to_string()),
///     endpoint_url: None,
///     allow_restricted: false,
///     prefetch_count: 16,  // Prefetch 16 blocks ahead
///     cache_capacity_bytes: Some(64 * 1024 * 1024),  // 64MB cache
/// };
///
/// let snap = open_snapshot(config).expect("Failed to open S3 snapshot");
/// ```
///
/// ## Opening from HTTP with SSRF Protection
///
/// ```rust,no_run
/// use strata_loader::engine::{OpenConfig, open_snapshot};
///
/// let config = OpenConfig {
///     path: "https://datasets.example.com/public/data.st".to_string(),
///     s3_region: None,
///     endpoint_url: None,
///     allow_restricted: false,  // Blocks connections to 10.0.0.0/8, etc.
///     prefetch_count: 8,
///     cache_capacity_bytes: Some(16 * 1024 * 1024),
/// };
///
/// match open_snapshot(config) {
///     Ok(snap) => println!("Opened remote snapshot"),
///     Err(e) => eprintln!("Failed to open: {}", e),
/// }
/// ```
///
/// # Performance Notes
///
/// - **Local Files**: Minimal overhead beyond OS page cache. Prefetching provides
///   diminishing returns on modern SSDs but significant gains on HDDs.
/// - **HTTP/HTTPS**: Prefetching is critical. Each request incurs ~50-200ms latency;
///   prefetch can improve throughput by 5-10x for sequential workloads.
/// - **S3**: Similar to HTTP but with additional authentication overhead per request.
///   Aggressive prefetching (16-32 blocks) recommended for training workloads.
///
/// # Thread Safety
///
/// The returned `Arc<StrataFile>` is `Send + Sync`. Multiple threads can safely
/// clone the `Arc` and issue concurrent reads. The internal cache and prefetch
/// pipeline are protected by locks and designed for high concurrency.
///
/// # Implementation Details
///
/// This function uses `strata_core::StrataFile::with_cache` to enable advanced
/// features like prefetching and custom cache sizes. The alternative
/// `StrataFile::new` would use default settings without prefetch support.
pub fn open_snapshot(config: OpenConfig) -> Result<Arc<StrataFile>, OpenError> {
    let backend: Arc<dyn StorageBackend> = if config.path.starts_with("http://")
        || config.path.starts_with("https://")
    {
        Arc::new(
            strata_core::store::http::HttpBackend::new(
                config.path.clone(),
                config.allow_restricted,
            )
            .map_err(|e| OpenError::Io(e.to_string()))?,
        )
    } else if config.path.starts_with("s3://") {
        let remainder = &config.path[5..];
        let parts: Vec<&str> = remainder.splitn(2, '/').collect();
        if parts.len() != 2 {
            return Err(OpenError::InvalidS3Uri(
                "Expected s3://bucket/key".to_string(),
            ));
        }
        let bucket = parts[0].to_string();
        let key = parts[1].to_string();
        let region = config.s3_region.unwrap_or_else(|| "us-east-1".to_string());

        Arc::new(
            strata_core::store::s3::AsyncS3Backend::new(bucket, key, region, config.endpoint_url)
                .map_err(|e| OpenError::Io(e.to_string()))?,
        )
    } else {
        Arc::new(
            strata_core::store::local::FileBackend::new(std::path::Path::new(&config.path))
                .map_err(|e| OpenError::Io(e.to_string()))?,
        )
    };

    let header_bytes = backend
        .read_exact(0, HEADER_SIZE)
        .map_err(|e| OpenError::Io(e.to_string()))?;

    let header: StrataHeader =
        bincode::deserialize(&header_bytes).map_err(|e| OpenError::InvalidHeader(e.to_string()))?;

    let compressor: Box<dyn Compressor> = match header.compression {
        CompressionType::Lz4 => Box::new(Lz4Compressor::new()),
        CompressionType::Zstd => Box::new(ZstdCompressor::new(DEFAULT_ZSTD_LEVEL, None)),
    };

    // Use with_cache to enable prefetching and custom cache size if configured
    let prefetch_window = if config.prefetch_count > 0 {
        Some(config.prefetch_count)
    } else {
        None
    };

    StrataFile::with_cache(
        backend,
        compressor,
        None,
        config.cache_capacity_bytes,
        prefetch_window,
    )
    .map_err(|e| OpenError::Io(e.to_string()))
}

/// Returns the total size in bytes of a specific stream in the snapshot.
///
/// Strata snapshots contain two independent streams:
/// - **Disk**: The virtual machine's persistent storage (e.g., root filesystem, data volumes).
/// - **Memory**: A snapshot of physical RAM at the time of capture (optional).
///
/// This function queries the stream's logical size without reading any data. The size
/// represents the decompressed byte count.
///
/// # Parameters
///
/// - `snap`: Reference to an opened snapshot (from [`open_snapshot`]).
/// - `stream`: Which stream to query (typically `SnapshotStream::Disk` for ML datasets).
///
/// # Returns
///
/// The stream size in bytes. If the stream is empty or not present, returns `0`.
///
/// # Examples
///
/// ```rust,no_run
/// use strata_loader::engine::{OpenConfig, open_snapshot, stream_size};
/// use strata_core::api::stratafile::SnapshotStream;
///
/// let config = OpenConfig {
///     path: "/data/snapshot.st".to_string(),
///     s3_region: None,
///     endpoint_url: None,
///     allow_restricted: false,
///     prefetch_count: 0,
///     cache_capacity_bytes: None,
/// };
///
/// let snap = open_snapshot(config).expect("Failed to open");
/// let disk_size = stream_size(&snap, SnapshotStream::Disk);
/// let mem_size = stream_size(&snap, SnapshotStream::Memory);
///
/// println!("Disk: {} bytes, Memory: {} bytes", disk_size, mem_size);
/// ```
///
/// # Performance
///
/// This operation reads only the snapshot header (cached after first access) and
/// completes in O(1) time. It does not perform any I/O on the data stream itself.
pub fn stream_size(snap: &StrataFile, stream: SnapshotStream) -> u64 {
    snap.size(stream)
}

/// Reads a contiguous range of bytes from a snapshot stream at an absolute offset.
///
/// This function performs a random-access read without modifying any cursor state.
/// It is the low-level primitive used by higher-level abstractions like iterators
/// and Python bindings.
///
/// # Decompression and Caching
///
/// Internally, Strata snapshots are divided into compressed blocks (typically 4KB).
/// When you request a byte range:
///
/// 1. The function determines which blocks contain the requested bytes.
/// 2. For each block, it checks the cache; if present, uses cached decompressed data.
/// 3. If not cached, reads the compressed block from the backend, decompresses it,
///    and stores it in the cache for future reuse.
/// 4. Copies the requested byte range from the decompressed blocks into a single
///    contiguous `Vec<u8>`.
///
/// This means that reading small ranges (e.g., 100 bytes) may decompress an entire
/// 4KB block, but subsequent reads from the same block will be served from cache.
///
/// # Prefetching
///
/// If `prefetch_count > 0` was set in [`OpenConfig`], this function triggers
/// asynchronous prefetching of subsequent blocks after the requested range. This
/// overlaps I/O with computation for sequential access patterns.
///
/// # Parameters
///
/// - `snap`: Shared reference to the snapshot (can be cloned across threads).
/// - `stream`: Which stream to read from (Disk or Memory).
/// - `offset`: Absolute byte offset from the start of the stream (0-indexed).
/// - `length`: Number of bytes to read.
///
/// # Returns
///
/// - `Ok(Vec<u8>)`: A freshly allocated vector containing the requested bytes.
/// - `Err(OpenError::Io)`: Read failed (offset beyond stream size, backend error, etc.).
///
/// # Errors
///
/// - Reading beyond the stream's size returns an error with the message indicating
///   the invalid offset or length.
/// - Decompression failures (due to data corruption) propagate as I/O errors.
/// - Backend errors (network timeout, disk read error) are wrapped in `OpenError::Io`.
///
/// # Examples
///
/// ## Reading the First 1KB of a Snapshot
///
/// ```rust,no_run
/// use strata_loader::engine::{OpenConfig, open_snapshot, read_stream};
/// use strata_core::api::stratafile::SnapshotStream;
///
/// let config = OpenConfig {
///     path: "/data/snapshot.st".to_string(),
///     s3_region: None,
///     endpoint_url: None,
///     allow_restricted: false,
///     prefetch_count: 0,
///     cache_capacity_bytes: None,
/// };
///
/// let snap = open_snapshot(config).expect("Failed to open");
/// let data = read_stream(&snap, SnapshotStream::Disk, 0, 1024)
///     .expect("Failed to read");
///
/// assert_eq!(data.len(), 1024);
/// ```
///
/// ## Reading with Offset for Random Access
///
/// ```rust,no_run
/// use strata_loader::engine::{OpenConfig, open_snapshot, read_stream};
/// use strata_core::api::stratafile::SnapshotStream;
///
/// let config = OpenConfig {
///     path: "s3://bucket/snapshot.st".to_string(),
///     s3_region: Some("us-east-1".to_string()),
///     endpoint_url: None,
///     allow_restricted: false,
///     prefetch_count: 4,  // Prefetch helps for sequential-ish access
///     cache_capacity_bytes: Some(16 * 1024 * 1024),
/// };
///
/// let snap = open_snapshot(config).expect("Failed to open");
///
/// // Read 4KB sample at offset 1MB
/// let sample = read_stream(&snap, SnapshotStream::Disk, 1024 * 1024, 4096)
///     .expect("Read failed");
/// ```
///
/// # Performance Characteristics
///
/// - **Time Complexity**: O(N) where N is the number of blocks spanning the range.
///   Typically O(1) for small reads within a single block.
/// - **Memory Allocation**: Allocates a new `Vec<u8>` of size `length`. For zero-copy
///   transfer to Python, use [`crate::tensor::numpy`] helpers instead.
/// - **Cache Hit**: ~100ns for in-cache reads (memcpy dominant).
/// - **Cache Miss (local SSD)**: ~10-50μs (read + decompress).
/// - **Cache Miss (network)**: ~50-200ms (latency dominant, mitigated by prefetch).
///
/// # Thread Safety
///
/// This function is thread-safe. Multiple threads can call `read_stream` on the same
/// `snap` concurrently. Reads do not interfere with each other, though they may
/// compete for cache space (LRU eviction).
pub fn read_stream(
    snap: &Arc<StrataFile>,
    stream: SnapshotStream,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, OpenError> {
    snap.read_at(stream, offset, length)
        .map_err(|e| OpenError::Io(e.to_string()))
}
