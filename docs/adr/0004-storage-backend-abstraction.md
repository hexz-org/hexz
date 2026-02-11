# 4. Storage Backend Abstraction Design

Date: Early development phase

## Status

Accepted

## Context

Strata must read compressed snapshots from diverse storage systems:

**ML Training Workflows**:
- **S3**: Primary storage for shared datasets (multi-region, versioned buckets)
- **Local NVMe**: Cached snapshots for fast iteration
- **HTTP/HTTPS**: Public datasets from CDNs and research institutions

**VM Boot Scenarios**:
- **Local Disk**: Production VM images on SSD/NVMe
- **NFS/CIFS**: Shared VM images in datacenters
- **HTTP**: Network boot scenarios

**Operational Requirements**:
- Transparent failover (S3 outage → HTTP mirror)
- Resume interrupted downloads
- Authentication (AWS SigV4, bearer tokens)
- Range request support for random access
- SSRF protection for user-provided URLs

Initial implementation used direct `std::fs::File` reads, preventing remote access. Adding S3 support revealed the need for abstraction: same read/seek API regardless of backend.

Design alternatives considered:

1. **Trait-based abstraction**: `StorageBackend` trait with `read_range()` method
2. **Enum with match statements**: `Backend::S3 | Backend::Local | Backend::Http`
3. **Dynamic dispatch**: `Box<dyn Read + Seek>` approach

The constraint is maintaining zero-copy performance for local files while supporting remote protocols.

## Decision

We will implement a **trait-based storage backend abstraction** with the following structure:

```rust
pub trait StorageBackend: Send + Sync {
    fn read_range(&self, offset: u64, length: usize) -> Result<Vec<u8>>;
    fn size(&self) -> Result<u64>;
    fn is_seekable(&self) -> bool;
}
```

### Backend Implementations

**`LocalFileBackend`**:
- Uses memory-mapped files (`mmap`) for zero-copy reads
- Falls back to `std::fs::File` for non-mmappable files
- Supports Linux, macOS, Windows

**`S3Backend`**:
- Built on `aws-sdk-s3` (official AWS SDK for Rust)
- Range requests via `GetObject` with `Range` header
- Credential chain: environment → profile → IAM role → IMDS
- Retry logic with exponential backoff
- Connection pooling via `hyper`

**`HttpBackend`**:
- Generic HTTP/HTTPS support using `reqwest`
- Follows redirects with configurable limit
- TLS certificate validation
- SSRF protection (blocks private IP ranges)
- Optional bearer token authentication

### URL Routing

The `open()` function parses URLs and selects the appropriate backend:

- `s3://bucket/key` → `S3Backend`
- `https://...` or `http://...` → `HttpBackend`
- `/path/to/file` or `file:///path` → `LocalFileBackend`

### Caching Layer

Remote backends automatically cache fetched blocks:
- LRU cache (default 256MB)
- Prefetch for sequential access patterns
- Cache persistence to disk (optional)

## Consequences

### Positive

- **Unified API**: Same code works for local and remote snapshots
- **Zero-Copy Local Reads**: Memory mapping for maximum performance (no syscall per read)
- **Transparent Remote Access**: ML engineers use `strata.open("s3://...")` without special handling
- **Testability**: Mock backend for unit tests without real S3/HTTP
- **Extensibility**: Adding new backends (GCS, Azure Blob, IPFS) requires implementing one trait
- **Connection Pooling**: HTTP(S) connections reused across requests
- **Security**: SSRF protection prevents reading from internal metadata servers

### Negative

- **Complexity**: Three implementations to maintain (local, S3, HTTP)
- **Error Handling**: Different failure modes (network timeout vs. disk error vs. 404)
- **Credential Management**: S3 credentials must be configured correctly (AWS CLI profile or env vars)
- **Latency Variance**: Remote reads have higher latency than local (mitigated by prefetching)
- **Memory Overhead**: Each backend instance holds state (HTTP client, S3 client, mmap handle)

### Neutral

- **Caching Strategy**: Remote backends cache aggressively; local backends skip cache
- **Buffer Allocation**: Remote reads allocate `Vec<u8>`; local reads return mmap slice
- **Retry Policy**: S3/HTTP retry with exponential backoff
- **Timeout Values**: Configurable timeout values for connections and reads
- **Region Handling**: S3 region auto-detected from bucket or explicit via `s3_region` parameter

## Future Enhancements

- **Write Backends**: Currently read-only; write support for uploading packed snapshots
- **Multi-Backend**: Try S3, fallback to HTTP mirror on 5xx errors
- **Streaming Prefetch**: Predict next blocks based on access pattern
- **Compression on Wire**: Negotiate gzip/brotli with HTTP servers to reduce bandwidth

## Related Decisions

- See explanation/storage-backend-design.md for implementation details
- See how-to/ml-workflows/setup-s3-streaming.md for S3 configuration guide
- See reference/configuration.md for backend options
