# Storage Backend Design

How Hexz abstracts over local files, S3, and HTTP.

## Design Goal

Provide unified API for reading snapshots regardless of storage location:
- Local files (`/path/to/file.st`)
- S3 buckets (`s3://bucket/key.st`)
- HTTP servers (`https://example.com/dataset.hxz`)

## Architecture

### Storage Backend Trait

```rust
pub trait StorageBackend: Send + Sync {
    fn read_range(&self, offset: u64, length: usize) -> Result<Vec<u8>>;
    fn size(&self) -> Result<u64>;
    fn is_seekable(&self) -> bool;
}
```

All backends implement this trait, providing uniform interface.

### URL Routing

The `open()` function parses URL and selects backend:

```rust
fn open(path: &str) -> Result<Box<dyn StorageBackend>> {
    if path.starts_with("s3://") {
        Ok(Box::new(S3Backend::new(path)?))
    } else if path.starts_with("http://") || path.starts_with("https://") {
        Ok(Box::new(HttpBackend::new(path)?))
    } else {
        Ok(Box::new(LocalFileBackend::new(path)?))
    }
}
```

## Backend Implementations

### LocalFileBackend

**Strategy**: Memory-mapped I/O for zero-copy reads

**Implementation**:
```rust
pub struct LocalFileBackend {
    mmap: Mmap,  // Memory-mapped file
    size: u64,
}

impl StorageBackend for LocalFileBackend {
    fn read_range(&self, offset: u64, length: usize) -> Result<Vec<u8>> {
        let start = offset as usize;
        let end = start + length;
        Ok(self.mmap[start..end].to_vec())  // Copy from mmap
    }
}
```

**Benefits**:
- Zero-copy (OS handles paging)
- Fast random access
- No network overhead

**Limitations**:
- Requires local file
- File must fit in virtual address space

### S3Backend

**Strategy**: Range requests via HTTP API

**Implementation**:
```rust
pub struct S3Backend {
    client: S3Client,
    bucket: String,
    key: String,
    cache: LruCache<u64, Vec<u8>>,  // Block cache
}

impl StorageBackend for S3Backend {
    fn read_range(&self, offset: u64, length: usize) -> Result<Vec<u8>> {
        // Check cache first
        if let Some(data) = self.cache.get(&offset) {
            return Ok(data.clone());
        }

        // Range request to S3
        let req = GetObjectRequest {
            bucket: self.bucket.clone(),
            key: self.key.clone(),
            range: Some(format!("bytes={}-{}", offset, offset + length - 1)),
            ..Default::default()
        };

        let response = self.client.get_object(req)?;
        let data = read_stream(response.body)?;

        // Cache result
        self.cache.put(offset, data.clone());

        Ok(data)
    }
}
```

**Benefits**:
- No local storage needed
- Scalable (S3 handles load)
- Built-in redundancy

**Limitations**:
- Network latency
- Bandwidth costs
- Requires credentials

**Optimizations**:
- LRU cache for frequently accessed blocks
- Connection pooling
- Retry logic with exponential backoff

### HttpBackend

**Strategy**: HTTP range requests

**Implementation**:
```rust
pub struct HttpBackend {
    client: reqwest::Client,
    url: String,
    cache: LruCache<u64, Vec<u8>>,
}

impl StorageBackend for HttpBackend {
    fn read_range(&self, offset: u64, length: usize) -> Result<Vec<u8>> {
        // Similar to S3Backend but uses HTTP Range header
        let response = self.client
            .get(&self.url)
            .header("Range", format!("bytes={}-{}", offset, offset + length - 1))
            .send()?;

        let data = response.bytes()?.to_vec();
        Ok(data)
    }
}
```

**Benefits**:
- Works with any HTTP server
- No special authentication
- CDN-friendly

**Limitations**:
- Server must support range requests
- No built-in retry logic (must implement)
- SSRF risk (mitigated with IP blocking)

## Caching Strategy

Remote backends use multi-level caching:

### L1: In-Memory LRU Cache

**Size**: Default 256MB, configurable

**Policy**: Least-Recently-Used eviction

**Benefit**: Sub-millisecond cache hits

### L2: Disk Cache (Optional)

**Location**: User-specified directory

**Policy**: Persistent across runs

**Benefit**: Fast subsequent epochs

**Implementation**:
```python
dataset = hexz.open(
    "s3://bucket/dataset.hxz",
    cache_size=512 * 1024 * 1024,  # 512MB in-memory
    cache_dir="/nvme/hexz-cache"  # Persistent disk cache
)
```

### Cache Key

Blocks cached by offset:
```rust
cache_key = (snapshot_id, block_offset)
```

This enables sharing cache across multiple readers of same snapshot.

## Authentication

### S3 Authentication

Follows AWS credential chain:
1. Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
2. AWS credentials file (`~/.aws/credentials`)
3. IAM role (EC2/ECS metadata service)

**Implementation**:
```rust
let credentials_provider = DefaultCredentialsProvider::new()?;
let client = S3Client::new_with(
    HttpClient::new()?,
    credentials_provider,
    region,
);
```

### HTTP Authentication

Optional bearer token:
```python
dataset = hexz.open(
    "https://private.example.com/dataset.hxz",
    auth_token="bearer_token_here"
)
```

## Error Handling

Different backends have different failure modes:

### Local File Errors
- File not found
- Permission denied
- Disk full (writing)
- I/O error (bad sectors)

### S3 Errors
- Network timeout
- 403 Forbidden (bad credentials)
- 404 Not Found
- 5xx server errors
- Throttling

### HTTP Errors
- Connection refused
- Timeout
- 4xx/5xx status codes
- Invalid redirect

**Retry Strategy**:
- Local: No retry (fail fast)
- S3/HTTP: Exponential backoff with configurable attempts

## SSRF Protection

HTTP backend blocks private IP ranges to prevent SSRF attacks:

```rust
fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            ipv4.is_private() ||
            ipv4.is_loopback() ||
            ipv4.is_link_local()
        }
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback() ||
            ipv6.is_unicast_link_local()
        }
    }
}
```

Prevents reading from:
- 10.0.0.0/8 (private networks)
- 172.16.0.0/12 (private networks)
- 192.168.0.0/16 (private networks)
- 169.254.0.0/16 (link-local, AWS metadata)
- 127.0.0.0/8 (loopback)

## Future Enhancements

**Planned**:
- Google Cloud Storage backend
- Azure Blob Storage backend
- Multi-backend failover (S3 primary, HTTP backup)
- Streaming writes (upload while packing)

**Under Consideration**:
- IPFS backend (content-addressed storage)
- BitTorrent backend (peer-to-peer distribution)
- Local network backends (NFS, CIFS)

## See Also

- [ADR-0004: Storage Backend Abstraction](../adr/0004-storage-backend-abstraction.md) - Design decisions
- [How-To: Setup S3 Streaming](../how-to/ml-workflows/setup-s3-streaming.md) - S3 configuration
- [Reference: Configuration](../reference/configuration.md) - Backend options
