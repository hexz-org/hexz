# strata-server

HTTP and NBD server for streaming Strata snapshot data over the network.

## Overview

`strata-server` provides network-facing interfaces for accessing compressed Strata snapshots via standard protocols. It enables remote access to snapshot data without requiring clients to download entire files, making it ideal for forensics, VM hosting, and distributed data access.

## Supported Protocols

### HTTP Range Server
Exposes snapshots via HTTP 1.1 with range request support (RFC 7233). Clients can fetch specific byte ranges using standard HTTP GET requests with `Range` headers.

### NBD (Network Block Device)
Allows mounting snapshots as Linux block devices using the NBD protocol. This enables transparent filesystem access and use of standard tools (mount, dd, fsck).

### S3 Gateway (Planned)
Future S3-compatible API for cloud integration (not yet implemented).

## Quick Examples

### Starting an HTTP Server

```rust
use std::sync::Arc;
use strata_core::StrataFile;
use strata_core::store::local::FileBackend;
use strata_core::algo::compression::lz4::Lz4Compressor;
use strata_server::serve_http;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let backend = Arc::new(FileBackend::new("snapshot.st".as_ref())?);
    let compressor = Box::new(Lz4Compressor::new());
    let snap = Arc::new(StrataFile::new(backend, compressor, None)?);

    // Start HTTP server on port 8080
    serve_http(snap, 8080).await?;
    Ok(())
}
```

### Starting an NBD Server

```rust
use std::sync::Arc;
use strata_core::StrataFile;
use strata_core::store::local::FileBackend;
use strata_core::algo::compression::lz4::Lz4Compressor;
use strata_server::serve_nbd;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let backend = Arc::new(FileBackend::new("snapshot.st".as_ref())?);
    let compressor = Box::new(Lz4Compressor::new());
    let snap = Arc::new(StrataFile::new(backend, compressor, None)?);

    // Start NBD server on port 10809
    serve_nbd(snap, 10809).await?;
    Ok(())
}
```

### Command-Line Usage

```bash
# Start HTTP server (via strata CLI)
strata sys serve --port 8080 snapshot.st

# Start NBD server
strata sys serve --nbd --port 10809 snapshot.st
```

## HTTP Server

### Endpoints

- **GET /disk** - Serves the disk stream (persistent storage)
- **GET /memory** - Serves the memory stream (RAM snapshot)

### Range Request Support

```bash
# Fetch first 4KB of disk stream
curl -H "Range: bytes=0-4095" http://localhost:8080/disk -o chunk.bin

# Fetch 1MB starting at offset 1MB
curl -H "Range: bytes=1048576-2097151" http://localhost:8080/memory -o chunk.bin

# Fetch from offset to EOF (clamped to 32 MiB)
curl -H "Range: bytes=1048576-" http://localhost:8080/disk -o large_chunk.bin
```

### Response Codes

- **206 Partial Content** - Successful range request
- **416 Range Not Satisfiable** - Invalid range or out of bounds
- **500 Internal Server Error** - Snapshot read failure

### Python Client Example

```python
import requests

# Fetch a range
headers = {'Range': 'bytes=0-4095'}
response = requests.get('http://localhost:8080/disk', headers=headers)
assert response.status_code == 206

# Parse Content-Range header
content_range = response.headers['Content-Range']
print(f"Content-Range: {content_range}")

data = response.content
print(f"Fetched {len(data)} bytes")
```

## NBD Server

### Client Usage (Linux)

```bash
# Connect NBD client to server
sudo nbd-client localhost 10809 /dev/nbd0

# Mount the block device (read-only)
sudo mount -o ro /dev/nbd0 /mnt/snapshot

# Access files normally
ls -la /mnt/snapshot
cat /mnt/snapshot/important.log

# Disconnect when done
sudo umount /mnt/snapshot
sudo nbd-client -d /dev/nbd0
```

### Use Cases

- **VM Hosting**: Boot VMs directly from NBD-mounted snapshots
- **Forensics**: Mount disk images for analysis without full download
- **File Access**: Browse snapshot contents with standard Linux tools

## Architecture

```
strata-server/
├── src/
│   ├── lib.rs          # HTTP server implementation (Axum)
│   └── nbd.rs          # NBD protocol implementation
```

Key design decisions:

- **Axum Framework**: Modern async HTTP server with excellent performance
- **Tokio Runtime**: Async I/O for handling thousands of concurrent connections
- **Localhost-Only**: Binds to 127.0.0.1 by default for security
- **DoS Protection**: Request size clamping (32 MiB max per request)

## Performance

### HTTP Server

| Metric | Value |
|--------|-------|
| Throughput (localhost) | 500-2000 MB/s |
| Latency (cache hit) | ~80 μs |
| Latency (cache miss) | ~1-5 ms |
| Concurrency | 1000+ connections |
| Memory per connection | ~100 KB |

### NBD Server

| Metric | Value |
|--------|-------|
| Throughput | 500-1000 MB/s |
| Latency | ~2-10 ms |
| Concurrency | Multiple clients |
| Memory per client | ~100 KB |

Performance is primarily limited by decompression CPU time, not network bandwidth.

## Security

### Current Security Posture

- **Localhost-only**: Binds to 127.0.0.1, not accessible from network
- **No authentication**: Anyone with local access can read snapshots
- **No TLS**: Plaintext protocols (acceptable for loopback)
- **DoS protection**: Request size clamping to prevent memory exhaustion

### Remote Access

For remote access, use:
- **SSH tunnel**: `ssh -L 8080:localhost:8080 user@server`
- **VPN**: WireGuard, OpenVPN, etc.
- **Reverse proxy**: nginx with TLS + authentication

### Future Enhancements (Planned)

- TLS/HTTPS support
- Bearer token authentication
- Rate limiting per IP
- Configurable bind addresses
- Audit logging

## DoS Protection

### Request Size Clamping

All HTTP requests are clamped to 32 MiB (`MAX_CHUNK_SIZE`) to prevent:
- Memory exhaustion from huge reads
- CPU exhaustion from excessive decompression

Example:
```text
Client requests:  Range: bytes=0-67108863   (64 MiB)
Server clamps to: Content-Range: bytes 0-33554431/total  (32 MiB)
```

Clients must check the `Content-Range` header and issue follow-up requests for remaining data.

## Protocol Limitations

### HTTP Server

**Supported:**
- Bounded ranges: `bytes=0-1023`
- Unbounded ranges: `bytes=1024-`

**Not supported:**
- Suffix ranges: `bytes=-1024` (last 1KB)
- Multi-part ranges: `bytes=0-100,200-300`

Returns HTTP 416 for unsupported range types.

### NBD Server

**Supported:**
- Read operations
- Block-level access
- Multiple concurrent clients

**Not supported:**
- Write operations (always read-only)
- Trim/discard commands

## Development

From the repository root:

```bash
# Build server crate
cargo build -p strata-server

# Run tests
cargo test -p strata-server

# Run HTTP server example
cargo run -p strata-server --example http_server
```

### Testing

```bash
# Unit tests
cargo test -p strata-server

# Integration tests with actual servers
cargo test -p strata-server --test integration
```

## Examples

The crate includes example programs:

```bash
# HTTP server example
cargo run --example http_server -- snapshot.st 8080

# NBD server example
cargo run --example nbd_server -- snapshot.st 10809
```

## Dependencies

- **axum**: HTTP server framework
- **tokio**: Async runtime
- **tower**: Middleware and utilities
- **strata-core**: Core snapshot engine

## See Also

- **[strata-core](../core/)** - Core engine (provides StrataFile)
- **[strata-cli](../cli/)** - CLI tool (serve command)
- **[User Documentation](../../docs/)** - Server configuration guides
- **[Project README](../../README.md)** - Main project overview
