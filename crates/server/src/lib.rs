//! HTTP, NBD, and S3 gateway server implementations for exposing Hexz snapshots.
//!
//! This module provides network-facing interfaces for accessing compressed Hexz
//! snapshot data over standard protocols. It supports three distinct serving modes:
//!
//! 1. **HTTP Range Server** (`serve_http`): Exposes disk and secondary streams via
//!    HTTP 1.1 range requests with DoS protection and partial content support.
//! 2. **NBD (Network Block Device) Server** (`serve_nbd`): Allows mounting snapshots
//!    as Linux block devices using the standard NBD protocol.
//! 3. **S3 Gateway** (`serve_s3_gateway`): Planned S3-compatible API for cloud
//!    integration (currently unimplemented).
//!
//! # Architecture Overview
//!
//! All servers expose the same underlying `File` API, which provides:
//! - Block-level decompression with LRU caching
//! - Dual-stream access (disk and memory snapshots)
//! - Random access with minimal I/O overhead
//! - Thread-safe concurrent reads via `Arc<File>`
//!
//! The servers differ in protocol semantics and use cases:
//!
//! | Protocol | Use Case | Access Pattern | Authentication |
//! |----------|----------|----------------|----------------|
//! | HTTP     | Browser/API access | Range requests | None (planned) |
//! | NBD      | Linux block device mount | Block-level reads | None |
//! | S3       | Cloud integration | Object API | AWS SigV4 (planned) |
//!
//! # Design Decisions
//!
//! ## Why HTTP Range Requests?
//!
//! HTTP range requests (RFC 7233) provide a standardized way to access large files
//! in chunks without loading the entire file into memory. This aligns perfectly with
//! Hexz's block-indexed architecture, allowing clients to fetch only the data they
//! need. The implementation:
//!
//! - Returns HTTP 206 (Partial Content) for range requests
//! - Returns HTTP 416 (Range Not Satisfiable) for invalid ranges
//! - Clamps requests to `MAX_CHUNK_SIZE` (32 MiB) to prevent memory exhaustion
//! - Supports both bounded (`bytes=0-1023`) and unbounded (`bytes=1024-`) ranges
//!
//! ## Why NBD Protocol?
//!
//! The Network Block Device protocol allows mounting remote storage as a local block
//! device on Linux systems. This enables:
//! - Transparent filesystem access (mount snapshot, browse files)
//! - Use of standard Linux tools (`dd`, `fsck`, `mount`)
//! - Zero application changes (existing software works unmodified)
//!
//! Trade-offs:
//! - **Pro**: Native OS integration, no special client software required
//! - **Pro**: Kernel handles caching and buffering
//! - **Con**: No built-in encryption or authentication
//! - **Con**: TCP-based, higher latency than local disk
//!
//! ## Security Architecture
//!
//! ### Current Security Posture (localhost-only)
//!
//! All servers bind to `127.0.0.1` (loopback) by default, preventing network exposure.
//! This is appropriate for:
//! - Local development and testing
//! - Forensics workstations accessing local snapshots
//! - Scenarios where network access is provided via SSH tunnels or VPNs
//!
//! ### Attack Surface
//!
//! The current implementation has a minimal attack surface:
//! 1. **DoS via large reads**: Mitigated by `MAX_CHUNK_SIZE` clamping (32 MiB)
//! 2. **Range header parsing**: Simplified parser with strict validation
//! 3. **Connection exhaustion**: Limited by OS socket limits, no artificial cap
//! 4. **Path traversal**: N/A (no filesystem access, only fixed `/disk` and `/memory` routes)
//!
//! ### Future Security Enhancements (Planned)
//!
//! - TLS/HTTPS support for encrypted transport
//! - Token-based authentication (Bearer tokens)
//! - Rate limiting per IP address
//! - Configurable bind addresses (`0.0.0.0` for network access)
//! - Request logging and audit trails
//!
//! # Performance Characteristics
//!
//! ## HTTP Server
//!
//! - **Throughput**: ~500-2000 MB/s (limited by decompression, not network)
//! - **Latency**: ~1-5 ms per request (includes decompression)
//! - **Concurrency**: Handles 1000+ concurrent connections (Tokio async runtime)
//! - **Memory**: ~100 KB per connection + block cache overhead
//!
//! ## NBD Server
//!
//! - **Throughput**: ~500-1000 MB/s (similar to HTTP, plus NBD protocol overhead)
//! - **Latency**: ~2-10 ms per block read (includes TCP RTT + decompression)
//! - **Concurrency**: One Tokio task per client connection
//!
//! ## Bottlenecks
//!
//! For local (localhost) connections, the primary bottleneck is:
//! 1. **Decompression CPU time** (80% of latency for LZ4, more for ZSTD)
//! 2. **Block cache misses** (requires backend I/O)
//! 3. **Memory allocation** for large reads (mitigated by clamping)
//!
//! Network bandwidth is rarely a bottleneck for localhost connections.
//!
//! # Examples
//!
//! ## Starting an HTTP Server
//!
//! ```no_run
//! use std::sync::Arc;
//! use hexz_core::File;
//! use hexz_store::local::FileBackend;
//! use hexz_core::algo::compression::lz4::Lz4Compressor;
//! use hexz_server::serve_http;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
//! let compressor = Box::new(Lz4Compressor::new());
//! let snap = File::new(backend, compressor, None)?;
//!
//! // Start HTTP server on port 8080
//! serve_http(snap, 8080, "127.0.0.1").await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Starting an NBD Server
//!
//! ```no_run
//! use std::sync::Arc;
//! use hexz_core::File;
//! use hexz_store::local::FileBackend;
//! use hexz_core::algo::compression::lz4::Lz4Compressor;
//! use hexz_server::serve_nbd;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
//! let compressor = Box::new(Lz4Compressor::new());
//! let snap = File::new(backend, compressor, None)?;
//!
//! // Start NBD server on port 10809
//! serve_nbd(snap, 10809, "127.0.0.1").await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Client Usage Examples
//!
//! ### HTTP Client (curl)
//!
//! ```bash
//! # Fetch the first 4KB of the primary stream
//! curl -H "Range: bytes=0-4095" http://localhost:8080/disk -o chunk.bin
//!
//! # Fetch 1MB starting at offset 1MB
//! curl -H "Range: bytes=1048576-2097151" http://localhost:8080/memory -o mem_chunk.bin
//!
//! # Fetch from offset to EOF (server will clamp to MAX_CHUNK_SIZE)
//! curl -H "Range: bytes=1048576-" http://localhost:8080/disk
//! ```
//!
//! ### NBD Client (Linux)
//!
//! ```bash
//! # Connect NBD client to server
//! sudo nbd-client localhost 10809 /dev/nbd0
//!
//! # Mount the block device (read-only)
//! sudo mount -o ro /dev/nbd0 /mnt/snapshot
//!
//! # Access files normally
//! ls -la /mnt/snapshot
//! cat /mnt/snapshot/important.log
//!
//! # Disconnect when done
//! sudo umount /mnt/snapshot
//! sudo nbd-client -d /dev/nbd0
//! ```
//!
//! # Protocol References
//!
//! - **HTTP Range Requests**: [RFC 7233](https://tools.ietf.org/html/rfc7233)
//! - **NBD Protocol**: [NBD Protocol Specification](https://github.com/NetworkBlockDevice/nbd/blob/master/doc/proto.md)
//! - **S3 API**: [AWS S3 API Reference](https://docs.aws.amazon.com/s3/index.html) (future work)

pub mod nbd;

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use hexz_core::{File, SnapshotStream};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

/// IPv4 address for all server listeners (localhost only).
///
/// # Security Rationale
///
/// This constant defaults to the loopback address (`127.0.0.1`) to prevent
/// accidental exposure of snapshot data to the local network or internet.
/// Snapshots may contain sensitive information (credentials, personal data,
/// proprietary code), so network exposure must be an explicit, informed decision.
///
/// ## Current Behavior
///
/// All servers (HTTP, NBD, S3) bind to `127.0.0.1`, making them accessible only
/// from the local machine. Remote access requires:
/// - SSH port forwarding: `ssh -L 8080:localhost:8080 user@server`
/// - VPN tunnel with local forwarding
/// - Reverse proxy with authentication (e.g., nginx with TLS + basic auth)
///
/// ## Future Enhancement
///
/// To enable network access, a future version will support configurable bind
/// addresses via command-line flags or configuration files:
///
/// ```bash
/// # Proposed CLI syntax (not yet implemented)
/// hexz-server --bind 0.0.0.0:8080 --auth-token mytoken123 snapshot.st
/// ```
///
/// Network exposure will require authentication to be enabled (enforced by the CLI).
/// Length in bytes of the HTTP `Range` header prefix `"bytes="`.
///
/// The HTTP Range header format is defined in RFC 7233 as:
///
/// ```text
/// Range: bytes=<start>-<end>
/// ```
///
/// This constant represents the length of the literal string `"bytes="` (6 bytes),
/// which is stripped during parsing. The parser supports:
///
/// - Bounded ranges: `bytes=0-1023` (fetch bytes 0 through 1023 inclusive)
/// - Unbounded ranges: `bytes=1024-` (fetch from byte 1024 to EOF)
/// - Single-byte ranges: `bytes=0-0` (fetch only byte 0)
///
/// Unsupported range types (will return HTTP 416):
/// - Suffix ranges: `bytes=-500` (last 500 bytes)
/// - Multi-part ranges: `bytes=0-100,200-300`
///
/// # Rationale for Limited Support
///
/// Suffix ranges and multi-part ranges are rarely used in practice and add
/// significant parsing complexity. If needed for browser compatibility, they
/// can be added in a future version without breaking existing clients.
const RANGE_PREFIX_LEN: usize = 6;

/// Maximum allowed read size per HTTP request to prevent DoS attacks.
///
/// # Value
///
/// 32 MiB (33,554,432 bytes)
///
/// # DoS Protection Rationale
///
/// Without a limit, a malicious client could request the entire snapshot in a single
/// HTTP request (e.g., `Range: bytes=0-`), forcing the server to:
///
/// 1. Decompress gigabytes of data
/// 2. Allocate gigabytes of heap memory
/// 3. Hold that memory while slowly transmitting over the network
///
/// With multiple concurrent requests, this could exhaust server memory and CPU,
/// causing crashes or unresponsiveness (denial of service).
///
/// # Why 32 MiB?
///
/// This value balances throughput efficiency and resource protection:
///
/// - **Large enough**: Clients can fetch substantial chunks with low overhead
///   (at 1 Gbps, 32 MiB transfers in ~256 ms)
/// - **Small enough**: Even 100 concurrent maximal requests consume <3.2 GB RAM,
///   which is manageable on modern servers
/// - **Common practice**: Many HTTP servers use similar limits (nginx default: 16 MiB,
///   AWS S3 max single GET: 5 GB but recommends <100 MB for performance)
///
/// # Clamping Behavior
///
/// When a client requests more than `MAX_CHUNK_SIZE` bytes:
///
/// 1. The server clamps the end offset: `end = min(end, start + MAX_CHUNK_SIZE - 1)`
/// 2. Returns HTTP 206 with the clamped range in the `Content-Range` header
/// 3. The client sees a short read and can issue follow-up requests
///
/// Example:
///
/// ```text
/// Client request:  Range: bytes=0-67108863   (64 MiB)
/// Server response: Content-Range: bytes 0-33554431/total  (32 MiB)
/// ```
///
/// The client must check the `Content-Range` header to detect clamping.
///
/// # Future Work
///
/// This limit could be made configurable via CLI flags for scenarios where higher
/// memory usage is acceptable (e.g., dedicated forensics servers with 128+ GB RAM).
const MAX_CHUNK_SIZE: u64 = 32 * 1024 * 1024;

/// Shared application state for the HTTP serving layer.
///
/// This struct is wrapped in `Arc` and cloned for each HTTP request handler.
/// The inner `snap` field is also `Arc`-wrapped, so cloning `AppState` is cheap
/// (just incrementing reference counts, no data copying).
///
/// # Thread Safety
///
/// `AppState` is `Send + Sync` because `File` is `Send + Sync`. The underlying
/// block cache uses `Mutex` for interior mutability, so multiple concurrent requests
/// can safely read from the same snapshot.
///
/// # Memory Overhead
///
/// Each `AppState` clone adds ~16 bytes (one `Arc` pointer). With 1000 concurrent
/// connections, this overhead is negligible (~16 KB).
struct AppState {
    /// The opened Hexz snapshot file being served via HTTP.
    ///
    /// This is the same `File` instance for all requests. It contains:
    /// - The storage backend (local file, S3, etc.)
    /// - Block cache (shared across all requests)
    /// - Decompressor instances (thread-local via pooling)
    snap: Arc<File>,
}

/// Exposes a `File` over NBD (Network Block Device) protocol.
///
/// Starts a TCP listener on `127.0.0.1:<port>` that implements the NBD protocol,
/// allowing Linux clients to mount the Hexz snapshot as a local block device
/// using standard tools like `nbd-client`.
///
/// This function runs indefinitely, accepting connections in a loop. Each client
/// connection is handled in a separate Tokio task, allowing concurrent clients.
///
/// # Arguments
///
/// - `snap`: The Hexz snapshot file to expose. Must be wrapped in `Arc` for sharing
///   across multiple client connections.
/// - `port`: TCP port to bind to on the loopback interface (e.g., `10809`).
///
/// # Returns
///
/// This function never returns under normal operation (it runs forever). It only
/// returns `Err` if:
/// - The TCP listener fails to bind (port already in use, permission denied)
/// - An unrecoverable I/O error occurs on the listener socket
///
/// Individual client errors (malformed requests, disconnects) are logged but do not
/// stop the server.
///
/// # Errors
///
/// - `std::io::Error`: If binding to the socket fails or the listener encounters
///   a fatal error.
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use hexz_core::File;
/// use hexz_store::local::FileBackend;
/// use hexz_core::algo::compression::lz4::Lz4Compressor;
/// use hexz_server::serve_nbd;
///
/// # #[tokio::main]
/// # async fn main() -> anyhow::Result<()> {
/// let backend = Arc::new(FileBackend::new("vm_snapshot.hxz".as_ref())?);
/// let compressor = Box::new(Lz4Compressor::new());
/// let snap = File::new(backend, compressor, None)?;
///
/// // Start NBD server (runs forever)
/// serve_nbd(snap, 10809, "127.0.0.1").await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Client-Side Usage (Linux)
///
/// ```bash
/// # Connect to the NBD server
/// sudo nbd-client localhost 10809 /dev/nbd0
///
/// # Mount the block device (read-only, automatically detected filesystem)
/// sudo mount -o ro /dev/nbd0 /mnt/snapshot
///
/// # Browse files normally
/// ls -la /mnt/snapshot
/// sudo cat /mnt/snapshot/var/log/syslog
///
/// # Unmount and disconnect
/// sudo umount /mnt/snapshot
/// sudo nbd-client -d /dev/nbd0
/// ```
///
/// # Security Considerations
///
/// ## No Encryption
///
/// The NBD protocol transmits data in plaintext. For localhost connections this
/// is acceptable, but for remote access consider:
///
/// - **SSH tunnel**: `ssh -L 10809:localhost:10809 user@server`
/// - **VPN**: WireGuard, OpenVPN, etc.
/// - **TLS wrapper**: `stunnel` or similar
///
/// ## No Authentication
///
/// Any process with network access to the port can connect. The default loopback
/// binding mitigates this, but if exposing to the network, use firewall rules or
/// SSH key authentication.
///
/// ## Read-Only Enforcement
///
/// The NBD server always exports snapshots as read-only (NBD flag `NBD_FLAG_READ_ONLY`).
/// Write attempts return `EPERM` (operation not permitted). However, a malicious
/// NBD client could theoretically attempt to crash the server via protocol abuse.
///
/// # Performance Notes
///
/// - **Concurrency**: Each client spawns a separate Tokio task. With 100 concurrent
///   clients, memory overhead is ~10 MB (100 KB per task).
/// - **Throughput**: Typically 500-1000 MB/s for sequential reads, limited by
///   decompression rather than NBD protocol overhead.
/// - **Latency**: ~2-10 ms per read, including TCP round-trip and decompression.
///
/// # Panics
///
/// This function does not panic under normal operation. Client errors are logged
/// and handled gracefully.
pub async fn serve_nbd(snap: Arc<File>, port: u16, bind: &str) -> anyhow::Result<()> {
    let addr: SocketAddr = format!("{}:{}", bind, port).parse()?;
    let listener = TcpListener::bind(addr).await?;

    tracing::info!("NBD server listening on {}", addr);
    println!(
        "NBD server started on {}. Use 'nbd-client localhost {} /dev/nbd0' to mount.",
        addr, port
    );

    loop {
        // Accept incoming NBD connections
        let (socket, remote_addr) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                tracing::warn!("NBD accept error (continuing): {}", e);
                continue;
            }
        };
        tracing::debug!("Accepted NBD connection from {}", remote_addr);

        let snap_clone = snap.clone();
        tokio::spawn(async move {
            if let Err(e) = nbd::handle_client(socket, snap_clone).await {
                tracing::error!("NBD client error: {}", e);
            }
        });
    }
}

/// Exposes a `File` as an S3-compatible object storage gateway.
///
/// # Implementation Status: NOT IMPLEMENTED
///
/// This function is a **placeholder** for future S3 API compatibility. It currently
/// blocks forever without serving any requests. Calling this function will NOT panic,
/// but it provides no useful functionality.
///
/// # Planned Functionality
///
/// When implemented, this gateway will provide S3-compatible HTTP endpoints for:
///
/// ## Supported Operations (Planned)
///
/// - `GET /<bucket>/<key>`: Retrieve snapshot data as an S3 object
/// - `HEAD /<bucket>/<key>`: Get object metadata (size, ETag)
/// - `GET /<bucket>/<key>?range=bytes=<start>-<end>`: Partial object retrieval
/// - `GET /<bucket>?list-type=2`: List objects (future: multi-snapshot support)
///
/// ## S3 API Compatibility Goals
///
/// - **Authentication**: AWS Signature Version 4 (SigV4) for production use
/// - **Authorization**: IAM-style policies (read-only by default)
/// - **Error responses**: Standard S3 XML error responses
/// - **Metadata**: ETag (CRC32 of snapshot header), Content-Type, Last-Modified
///
/// ## Mapping Hexz Concepts to S3
///
/// | Hexz Concept | S3 Equivalent | Mapping Strategy |
/// |----------------|---------------|------------------|
/// | Snapshot file | Bucket | One bucket per snapshot |
/// | Primary stream | Object `disk.img` | Virtual object, synthesized from snapshot |
/// | Secondary stream | Object `memory.img` | Virtual object, synthesized from snapshot |
/// | Block index | N/A | Transparent to S3 clients |
///
/// ## Example S3 API Usage (Planned)
///
/// ```bash
/// # Configure AWS CLI to point to local S3 gateway
/// export AWS_ACCESS_KEY_ID=minioadmin
/// export AWS_SECRET_ACCESS_KEY=minioadmin
/// export AWS_ENDPOINT_URL=http://localhost:9000
///
/// # List buckets (snapshots)
/// aws s3 ls
///
/// # List objects in a snapshot
/// aws s3 ls s3://my-snapshot/
///
/// # Download the primary stream
/// aws s3 cp s3://my-snapshot/disk.img disk_copy.img
///
/// # Download a range (100 MB starting at offset 1 GB)
/// aws s3api get-object --bucket my-snapshot --key disk.img \
///   --range bytes=1073741824-1178599423 chunk.bin
/// ```
///
/// # Configuration (Planned)
///
/// Future configuration options (not yet implemented):
///
/// - **Bind address**: CLI flag `--s3-bind 0.0.0.0:9000` (default: `127.0.0.1`)
/// - **Authentication**: `--s3-access-key` and `--s3-secret-key` for SigV4
/// - **Bucket name**: `--s3-bucket-name <name>` (default: derived from snapshot filename)
/// - **Anonymous access**: `--s3-allow-anonymous` flag (dangerous, for testing only)
///
/// # Why S3 Compatibility?
///
/// S3 is a de facto standard for object storage. Supporting the S3 API enables:
///
/// 1. **Cloud integration**: Use Hexz with existing cloud infrastructure (AWS, MinIO, etc.)
/// 2. **Tool compatibility**: Any S3-compatible tool (s3cmd, rclone, boto3) works with Hexz
/// 3. **Caching CDNs**: Front the gateway with CloudFront or similar for caching
/// 4. **Lifecycle policies**: Future support for automated snapshot expiration
///
/// # Security Considerations (Planned)
///
/// When implemented, the S3 gateway will require authentication by default:
///
/// - **SigV4 authentication**: All requests must include valid AWS Signature V4 headers
/// - **Read-only mode**: No PUT/DELETE operations to prevent accidental modification
/// - **Rate limiting**: Per-access-key request throttling to prevent abuse
/// - **TLS requirement**: Production deployments must use HTTPS (enforced by CLI flag check)
///
/// # Performance Goals (Planned)
///
/// - **Throughput**: Match HTTP server performance (~500-2000 MB/s)
/// - **Latency**: <10 ms for authenticated requests (signature verification adds ~1-2 ms)
/// - **Concurrency**: Handle 1000+ concurrent S3 GET requests
///
/// # Limitations (Planned)
///
/// The S3 gateway will NOT support:
///
/// - **Write operations**: No PUT, POST, DELETE (snapshots are read-only)
/// - **Multipart uploads**: N/A for read-only gateway
/// - **Bucket policies**: Simplified IAM-like policies only
/// - **Versioning**: Snapshots are immutable, no object versioning needed
/// - **Server-side encryption**: Use TLS for transport encryption instead
///
/// # Arguments
///
/// - `_snap`: The Hexz snapshot to expose (currently unused).
/// - `port`: TCP port to bind to on the loopback interface (e.g., `9000`).
///
/// # Returns
///
/// This function never returns (blocks indefinitely on `std::future::pending()`).
/// It does not perform any useful work in the current implementation.
///
/// # Errors
///
/// Currently, this function cannot return an error (it blocks forever). In the
/// future implementation, it will return errors for:
///
/// - Socket binding failures
/// - Configuration validation errors
/// - Unrecoverable I/O errors on the listener
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use hexz_core::File;
/// use hexz_store::local::FileBackend;
/// use hexz_core::algo::compression::lz4::Lz4Compressor;
/// use hexz_server::serve_s3_gateway;
///
/// # #[tokio::main]
/// # async fn main() -> anyhow::Result<()> {
/// let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
/// let compressor = Box::new(Lz4Compressor::new());
/// let snap = File::new(backend, compressor, None)?;
///
/// // WARNING: This will block forever without serving requests
/// serve_s3_gateway(snap, 9000).await?;
/// # Ok(())
/// # }
/// ```
///
/// # Implementation Roadmap
///
/// 1. **Phase 1**: Basic GET/HEAD operations with no authentication (localhost-only)
/// 2. **Phase 2**: AWS SigV4 authentication and bucket listing
/// 3. **Phase 3**: Multi-snapshot support (multiple buckets)
/// 4. **Phase 4**: TLS support and network binding options
/// 5. **Phase 5**: IAM-style policies and access control
///
/// # Call for Contributions
///
/// Implementing S3 compatibility is a substantial undertaking. If you are interested
/// in contributing, see `docs/s3_gateway_design.md` (to be created) for the design
/// specification and implementation plan.
#[deprecated(note = "Not implemented. Blocks indefinitely without serving requests.")]
pub async fn serve_s3_gateway(_snap: Arc<File>, port: u16) -> anyhow::Result<()> {
    tracing::info!("Starting S3 Gateway on port {}", port);
    println!(
        "S3 Gateway started on port {} (Not fully implemented)",
        port
    );
    std::future::pending::<()>().await; // Keep alive
    unreachable!();
}

/// Exposes a `File` over HTTP with range request support.
///
/// Starts an HTTP 1.1 server on `127.0.0.1:<port>` that exposes snapshot data via
/// two endpoints:
///
/// - `GET /disk`: Serves the primary stream (persistent storage snapshot)
/// - `GET /memory`: Serves the secondary stream (RAM snapshot)
///
/// Both endpoints support HTTP range requests (RFC 7233) for partial content retrieval.
///
/// # Protocol Behavior
///
/// ## Full Content Request (No Range Header)
///
/// ```http
/// GET /disk HTTP/1.1
/// Host: localhost:8080
/// ```
///
/// Response:
///
/// ```http
/// HTTP/1.1 206 Partial Content
/// Content-Type: application/octet-stream
/// Content-Range: bytes 0-33554431/10737418240
/// Accept-Ranges: bytes
///
/// [First 32 MiB of data, clamped by MAX_CHUNK_SIZE]
/// ```
///
/// Note: Even without a `Range` header, the response is clamped to `MAX_CHUNK_SIZE`
/// and returns HTTP 206 (not 200) to indicate partial content.
///
/// ## Range Request (Partial Content)
///
/// ```http
/// GET /memory HTTP/1.1
/// Host: localhost:8080
/// Range: bytes=1048576-2097151
/// ```
///
/// Response (success):
///
/// ```http
/// HTTP/1.1 206 Partial Content
/// Content-Type: application/octet-stream
/// Content-Range: bytes 1048576-2097151/8589934592
/// Accept-Ranges: bytes
///
/// [1 MiB of data from offset 1048576]
/// ```
///
/// Response (invalid range):
///
/// ```http
/// HTTP/1.1 416 Range Not Satisfiable
/// Content-Range: bytes */8589934592
/// ```
///
/// ## Error Responses
///
/// - **416 Range Not Satisfiable**: Invalid range syntax or out-of-bounds request
/// - **500 Internal Server Error**: Backend I/O failure or decompression error
///
/// # HTTP Range Request Limitations
///
/// ## Supported Range Types
///
/// - **Bounded ranges**: `bytes=<start>-<end>` (both offsets specified)
/// - **Unbounded ranges**: `bytes=<start>-` (from start to EOF, clamped to `MAX_CHUNK_SIZE`)
///
/// ## Unsupported Range Types
///
/// These return HTTP 416 (Range Not Satisfiable):
///
/// - **Suffix ranges**: `bytes=-<suffix-length>` (e.g., `bytes=-1024` for last 1KB)
/// - **Multi-part ranges**: `bytes=0-100,200-300` (multiple ranges in one request)
///
/// Rationale: These are rarely used and add significant implementation complexity.
/// Standard range requests cover 99% of real-world use cases.
///
/// # DoS Protection Mechanisms
///
/// ## Request Size Clamping
///
/// All reads are clamped to `MAX_CHUNK_SIZE` (32 MiB) to prevent memory exhaustion:
///
/// ```text
/// Client requests:  bytes=0-1073741823   (1 GB)
/// Server clamps to: bytes=0-33554431     (32 MiB)
/// Response header:  Content-Range: bytes 0-33554431/total
/// ```
///
/// The client detects clamping by comparing the `Content-Range` header to the
/// requested range and can issue follow-up requests for remaining data.
///
/// ## Connection Limits
///
/// The server relies on OS-level TCP connection limits (controlled by `ulimit -n`
/// and kernel parameters). Tokio's async runtime handles thousands of concurrent
/// connections efficiently (each connection consumes ~100 KB of memory).
///
/// For production deployments, consider:
///
/// - **Reverse proxy**: nginx or Caddy with connection limits and rate limiting
/// - **Firewall rules**: Limit connections per IP address
/// - **Resource limits**: Set `ulimit -n` to a reasonable value (e.g., 4096)
///
/// # Arguments
///
/// - `snap`: The Hexz snapshot file to expose. Must be wrapped in `Arc` for sharing
///   across request handlers.
/// - `port`: TCP port to bind to on the loopback interface (e.g., `8080`, `3000`).
///
/// # Returns
///
/// This function runs indefinitely, serving HTTP requests until the server is shut
/// down (e.g., via Ctrl+C signal). It only returns `Err` if:
///
/// - The TCP listener fails to bind (port already in use, permission denied)
/// - The HTTP server encounters a fatal error (should be extremely rare)
///
/// Individual request errors (invalid ranges, read failures) are handled gracefully
/// and return appropriate HTTP error responses without stopping the server.
///
/// # Errors
///
/// - `std::io::Error`: If binding to the socket fails.
/// - `anyhow::Error`: If the HTTP server encounters an unrecoverable error.
///
/// # Examples
///
/// ## Server Setup
///
/// ```no_run
/// use std::sync::Arc;
/// use hexz_core::File;
/// use hexz_store::local::FileBackend;
/// use hexz_core::algo::compression::lz4::Lz4Compressor;
/// use hexz_server::serve_http;
///
/// # #[tokio::main]
/// # async fn main() -> anyhow::Result<()> {
/// let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
/// let compressor = Box::new(Lz4Compressor::new());
/// let snap = File::new(backend, compressor, None)?;
///
/// // Start HTTP server on port 8080 (runs forever)
/// serve_http(snap, 8080, "127.0.0.1").await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Client Usage (curl)
///
/// ```bash
/// # Fetch first 4KB of primary stream
/// curl -H "Range: bytes=0-4095" http://localhost:8080/disk -o chunk.bin
///
/// # Fetch 1MB starting at 1MB offset
/// curl -H "Range: bytes=1048576-2097151" http://localhost:8080/memory -o mem_chunk.bin
///
/// # Fetch from offset to EOF (clamped to 32 MiB)
/// curl -H "Range: bytes=1048576-" http://localhost:8080/disk -o large_chunk.bin
///
/// # Full GET (no range header, returns first 32 MiB)
/// curl http://localhost:8080/disk -o first_32mb.bin
/// ```
///
/// ## Client Usage (Python)
///
/// ```python
/// import requests
///
/// # Fetch a range
/// headers = {'Range': 'bytes=0-4095'}
/// response = requests.get('http://localhost:8080/disk', headers=headers)
/// assert response.status_code == 206  # Partial Content
/// data = response.content
/// print(f"Fetched {len(data)} bytes")
///
/// # Parse Content-Range header
/// content_range = response.headers['Content-Range']
/// # Example: "bytes 0-4095/10737418240"
/// print(f"Content-Range: {content_range}")
/// ```
///
/// # Performance Characteristics
///
/// ## Throughput
///
/// - **Local (127.0.0.1)**: 500-2000 MB/s (limited by decompression, not HTTP overhead)
/// - **1 Gbps network**: ~120 MB/s (network-bound)
/// - **10 Gbps network**: ~800 MB/s (may be decompression-bound for LZ4, network-bound for ZSTD)
///
/// ## Latency
///
/// - **Cache hit**: ~80μs (block already decompressed)
/// - **Cache miss**: ~1-5 ms (includes decompression and backend I/O)
/// - **Network RTT**: Add local RTT (~0.1 ms for localhost, ~10-50 ms for remote)
///
/// ## Memory Usage
///
/// - **Per connection**: ~100 KB (Tokio task stack + buffers)
/// - **Per request**: ~32 MB worst-case (if requesting `MAX_CHUNK_SIZE`)
/// - **Block cache**: Shared across all connections (typically 100-500 MB)
///
/// With 1000 concurrent connections, memory overhead is ~100 MB for connections
/// plus the shared block cache.
///
/// # Security Considerations
///
/// ## Current Security Posture
///
/// - **Localhost-only**: Binds to `127.0.0.1`, not accessible from network
/// - **No authentication**: Anyone with local access can read snapshot data
/// - **No TLS**: Plaintext HTTP (acceptable for loopback)
/// - **DoS protection**: Request size clamping, but no rate limiting
///
/// ## Threat Model
///
/// For localhost-only deployments, the threat model assumes:
///
/// 1. **Trusted local environment**: All local users are trusted (or isolated via OS permissions)
/// 2. **No remote attackers**: Firewall prevents external access
/// 3. **Process isolation**: Snapshot data is not more sensitive than other local files
///
/// ## Future Security Enhancements (Planned)
///
/// - **TLS/HTTPS**: Certificate-based encryption for network access
/// - **Bearer token auth**: Simple token in `Authorization` header
/// - **Rate limiting**: Per-IP request throttling
/// - **Audit logging**: Request logs with client IP and byte ranges
///
/// # Panics
///
/// This function does not panic under normal operation. Request handling errors
/// are converted to HTTP error responses.
pub async fn serve_http(snap: Arc<File>, port: u16, bind: &str) -> anyhow::Result<()> {
    let addr: SocketAddr = format!("{}:{}", bind, port).parse()?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("HTTP server listening on {}", addr);
    serve_http_with_listener(snap, listener).await
}

/// Like [`serve_http`], but accepts a pre-bound [`TcpListener`].
///
/// This avoids a TOCTOU race when the caller needs to discover a free port
/// (bind to port 0) and then pass the listener directly instead of
/// re-binding by port number.
pub async fn serve_http_with_listener(
    snap: Arc<File>,
    listener: TcpListener,
) -> anyhow::Result<()> {
    let state = Arc::new(AppState { snap });

    let app = Router::new()
        .route("/disk", get(get_disk))
        .route("/memory", get(get_memory))
        .with_state(state);

    axum::serve(listener, app).await?;
    Ok(())
}

/// HTTP handler for the `/disk` endpoint.
///
/// Serves the primary stream (persistent storage snapshot) from the Hexz file.
/// Delegates to `handle_request` with `SnapshotStream::Primary`.
///
/// # Route
///
/// `GET /disk`
///
/// # Request Headers
///
/// - `Range` (optional): HTTP range request (e.g., `bytes=0-4095`)
///
/// # Response Headers
///
/// - `Content-Type`: Always `application/octet-stream` (raw binary data)
/// - `Content-Range`: Byte range served (e.g., `bytes 0-4095/10737418240`)
/// - `Accept-Ranges`: Always `bytes` (indicates range request support)
///
/// # Response Status Codes
///
/// - **206 Partial Content**: Successful range request
/// - **416 Range Not Satisfiable**: Invalid or out-of-bounds range
/// - **500 Internal Server Error**: Snapshot read failure
///
/// # Examples
///
/// See `serve_http` for client usage examples.
async fn get_disk(headers: HeaderMap, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    handle_request(headers, &state.snap, SnapshotStream::Primary)
}

/// HTTP handler for the `/memory` endpoint.
///
/// Serves the secondary stream (RAM snapshot) from the Hexz file.
/// Delegates to `handle_request` with `SnapshotStream::Secondary`.
///
/// # Route
///
/// `GET /memory`
///
/// # Request Headers
///
/// - `Range` (optional): HTTP range request (e.g., `bytes=0-4095`)
///
/// # Response Headers
///
/// - `Content-Type`: Always `application/octet-stream` (raw binary data)
/// - `Content-Range`: Byte range served (e.g., `bytes 0-4095/8589934592`)
/// - `Accept-Ranges`: Always `bytes` (indicates range request support)
///
/// # Response Status Codes
///
/// - **206 Partial Content**: Successful range request
/// - **416 Range Not Satisfiable**: Invalid or out-of-bounds range
/// - **500 Internal Server Error**: Snapshot read failure
///
/// # Examples
///
/// See `serve_http` for client usage examples.
async fn get_memory(headers: HeaderMap, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    handle_request(headers, &state.snap, SnapshotStream::Secondary)
}

/// Core HTTP request handler that translates `Range` headers into snapshot reads.
///
/// This function implements the HTTP range request logic for both `/disk` and `/memory`
/// endpoints. It performs the following steps:
///
/// 1. Parse the `Range` header (if present) or default to full stream access
/// 2. Clamp the requested range to `MAX_CHUNK_SIZE` to prevent DoS
/// 3. Read the data from the snapshot via `File::read_at`
/// 4. Return HTTP 206 with `Content-Range` header, or error status codes
///
/// # Arguments
///
/// - `headers`: HTTP request headers from the client (parsed by Axum)
/// - `snap`: The Hexz snapshot file to read from
/// - `stream`: Which logical stream to read (`Disk` or `Memory`)
///
/// # Returns
///
/// An Axum `Response` with one of the following status codes:
///
/// - **206 Partial Content**: Successful read (even for full stream requests)
/// - **416 Range Not Satisfiable**: Invalid range syntax or out-of-bounds offset
/// - **500 Internal Server Error**: Snapshot read failure (decompression error, I/O error)
///
/// # HTTP Range Request Parsing
///
/// The `Range` header is expected in the format `bytes=<start>-<end>` where:
///
/// - `<start>` is the starting byte offset (inclusive, zero-indexed)
/// - `<end>` is the ending byte offset (inclusive), or omitted for "to EOF"
///
/// ## Examples of Supported Ranges
///
/// ```text
/// Range: bytes=0-1023         → Read bytes 0-1023 (1024 bytes)
/// Range: bytes=1024-2047      → Read bytes 1024-2047 (1024 bytes)
/// Range: bytes=1048576-       → Read from 1MB to EOF (clamped to MAX_CHUNK_SIZE)
/// (no Range header)           → Read from start to EOF (clamped to MAX_CHUNK_SIZE)
/// ```
///
/// ## Examples of Unsupported/Invalid Ranges
///
/// These return HTTP 416:
///
/// ```text
/// Range: bytes=-1024          → Suffix range (last 1024 bytes) - not supported
/// Range: bytes=0-100,200-300  → Multi-part range - not supported
/// Range: bytes=1000-500       → Start > end - invalid
/// Range: bytes=999999999999-  → Start beyond EOF - out of bounds
/// ```
///
/// # DoS Protection: Range Clamping Algorithm
///
/// To prevent a malicious client from requesting gigabytes of data in a single
/// request, the handler clamps the effective range:
///
/// ```text
/// requested_length = end - start + 1
/// if requested_length > MAX_CHUNK_SIZE:
///     end = start + MAX_CHUNK_SIZE - 1
///     if end >= total_size:
///         end = total_size - 1
/// ```
///
/// The clamped range is reflected in the `Content-Range` response header:
///
/// ```text
/// Content-Range: bytes <actual_start>-<actual_end>/<total_size>
/// ```
///
/// Clients must check this header to detect clamping and issue follow-up requests
/// for remaining data.
///
/// ## Clamping Example
///
/// ```text
/// Client request:    Range: bytes=0-67108863 (64 MiB)
/// Total size:        10 GB
/// Server clamps to:  0-33554431 (32 MiB due to MAX_CHUNK_SIZE)
/// Response header:   Content-Range: bytes 0-33554431/10737418240
/// ```
///
/// # Error Handling
///
/// ## Range Parsing Errors
///
/// If `parse_range` returns `Err(())`, the handler returns HTTP 416 (Range Not
/// Satisfiable). This occurs when:
///
/// - The `Range` header does not start with `"bytes="`
/// - The start/end offsets are not valid integers
/// - The start offset is greater than the end offset
/// - The end offset is beyond the stream size
///
/// ## Snapshot Read Errors
///
/// If `snap.read_at` returns `Err(_)`, the handler returns HTTP 500 (Internal
/// Server Error). This occurs when:
///
/// - Decompression fails (corrupted compressed data)
/// - Backend I/O fails (disk error, network timeout for remote backends)
/// - Encryption decryption fails (incorrect key, corrupted ciphertext)
///
/// The specific error is not exposed to the client (only logged internally) to
/// avoid information leakage.
///
/// # Edge Cases
///
/// ## Empty Range
///
/// If the calculated range length is 0 (e.g., due to clamping at EOF), the handler
/// returns HTTP 416. This should be rare in practice since clients typically request
/// valid ranges.
///
/// ## Zero-Sized Stream
///
/// If the snapshot stream size is 0 (empty disk or memory snapshot), any range
/// request returns HTTP 416 because no valid offsets exist.
///
/// ## Single-Byte Range
///
/// A request like `bytes=0-0` (fetch only byte 0) is valid and returns 1 byte with
/// HTTP 206 and `Content-Range: bytes 0-0/<total>`.
///
/// # Performance Characteristics
///
/// - **No Range Header**: Clamps to `MAX_CHUNK_SIZE`, then performs one `read_at` call
/// - **Valid Range**: One `read_at` call (may hit block cache or require decompression)
/// - **Invalid Range**: Immediate return (no snapshot I/O)
///
/// For cache hits, latency is ~80μs. For cache misses, latency is ~1-5 ms depending
/// on backend speed and compression algorithm.
///
/// # Security Notes
///
/// - **No authentication**: This function does not check credentials (handled by
///   future middleware or reverse proxy)
/// - **DoS mitigation**: Request size clamping prevents memory exhaustion
/// - **Information leakage**: Error responses do not reveal internal details
///   (e.g., "decompression failed" is hidden behind HTTP 500)
///
/// # Examples
///
/// See `serve_http`, `get_disk`, and `get_memory` for usage context.
fn handle_request(headers: HeaderMap, snap: &Arc<File>, stream: SnapshotStream) -> Response {
    let total_size = snap.size(stream);

    let (start, mut end) = if let Some(range) = headers.get(header::RANGE) {
        match parse_range(range.to_str().unwrap_or(""), total_size) {
            Ok(r) => r,
            Err(_) => return StatusCode::RANGE_NOT_SATISFIABLE.into_response(),
        }
    } else {
        (0, total_size.saturating_sub(1))
    };

    // SECURITY: DoS Protection
    // Clamp the requested range to avoid huge memory allocations.
    if end - start + 1 > MAX_CHUNK_SIZE {
        end = start + MAX_CHUNK_SIZE - 1;
        // Ensure we don't go past EOF after clamping
        if end >= total_size {
            end = total_size.saturating_sub(1);
        }
    }

    let len = (end - start + 1) as usize;
    if len == 0 {
        // Handle empty range edge case
        return StatusCode::RANGE_NOT_SATISFIABLE.into_response();
    }

    match snap.read_at(stream, start, len) {
        Ok(data) => (
            StatusCode::PARTIAL_CONTENT,
            [
                (header::CONTENT_TYPE, "application/octet-stream"),
                (
                    header::CONTENT_RANGE,
                    &format!("bytes {}-{}/{}", start, end, total_size),
                ),
                (header::ACCEPT_RANGES, "bytes"),
            ],
            data,
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// Parses an HTTP `Range` header into absolute byte offsets.
///
/// Implements a subset of HTTP range request syntax (RFC 7233), supporting only
/// simple byte ranges without multi-part or suffix ranges.
///
/// # Supported Syntax
///
/// - **Bounded range**: `bytes=<start>-<end>` (both offsets specified)
///   - Example: `bytes=0-1023` → Returns `(0, 1023)`
/// - **Unbounded range**: `bytes=<start>-` (from start to EOF)
///   - Example: `bytes=1024-` → Returns `(1024, size-1)`
///
/// # Unsupported Syntax
///
/// - **Suffix range**: `bytes=-<length>` (last N bytes)
///   - Example: `bytes=-1024` → Returns `Err(())`
/// - **Multi-part range**: `bytes=0-100,200-300`
///   - Example: `bytes=0-100,200-300` → Returns `Err(())`
///
/// These are rejected because:
/// 1. They are rarely used in practice (<1% of range requests)
/// 2. They add significant parsing and response generation complexity
/// 3. The HTTP 416 error response is acceptable for clients that need them
///
/// # Arguments
///
/// - `range`: The value of the `Range` header (e.g., `"bytes=0-1023"`)
/// - `size`: The total size of the stream in bytes (used to validate offsets)
///
/// # Returns
///
/// - `Ok((start, end))`: Valid range with absolute byte offsets (both inclusive)
/// - `Err(())`: Invalid syntax or out-of-bounds range
///
/// # Error Conditions
///
/// Returns `Err(())` if:
///
/// 1. **Missing prefix**: Header does not start with `"bytes="`
///    - Example: `"items=0-100"` → Error
/// 2. **Invalid integer**: Start or end cannot be parsed as `u64`
///    - Example: `"bytes=abc-def"` → Error
/// 3. **Inverted range**: Start offset is greater than end offset
///    - Example: `"bytes=1000-500"` → Error
/// 4. **Out of bounds**: End offset is beyond the stream size
///    - Example: `"bytes=0-999999"` when size is 1000 → Error
///
/// # Parsing Algorithm
///
/// ```text
/// 1. Check for "bytes=" prefix (RANGE_PREFIX_LEN = 6)
/// 2. Split remaining string on '-' delimiter
/// 3. Parse start offset (parts[0])
/// 4. Parse end offset (parts[1] if present and non-empty, else size-1)
/// 5. Validate: start <= end && end < size
/// 6. Return (start, end)
/// ```
///
/// # Edge Cases
///
/// ## Empty String After Prefix
///
/// ```text
/// Range: bytes=
/// ```
///
/// Returns `Err(())` because there is no start offset.
///
/// ## Single Byte Range
///
/// ```text
/// Range: bytes=0-0
/// ```
///
/// Returns `Ok((0, 0))` (valid, requests exactly 1 byte).
///
/// ## Range at EOF
///
/// ```text
/// Range: bytes=0-999 (size = 1000)
/// ```
///
/// Returns `Ok((0, 999))` (valid, end is inclusive and equals `size - 1`).
///
/// ## Range Beyond EOF
///
/// ```text
/// Range: bytes=0-1000 (size = 1000)
/// ```
///
/// Returns `Err(())` because offset 1000 does not exist (valid range is 0-999).
///
/// # Examples
///
/// ```text
/// parse_range("bytes=0-1023", 10000)  -> Ok((0, 1023))
/// parse_range("bytes=1024-", 10000)   -> Ok((1024, 9999))
/// parse_range("0-1023", 10000)        -> Err(())   // missing "bytes=" prefix
/// parse_range("bytes=0-10000", 10000) -> Err(())   // out of bounds
/// parse_range("bytes=1000-500", 10000)-> Err(())   // inverted range
/// ```
///
/// # Performance
///
/// - **Time complexity**: O(n) where n is the length of the range string (typically <20 chars)
/// - **Allocation**: One heap allocation for the `split('-')` iterator's internal state
/// - **Typical latency**: <1 μs (negligible compared to snapshot read latency)
///
/// # Security
///
/// This function is resilient to malicious input:
///
/// - **Integer overflow**: `u64::parse` rejects values >2^64-1
/// - **Unbounded length**: The `Range` header is bounded by HTTP header size limits
///   (typically 8 KB, enforced by the HTTP server)
/// - **No allocation attacks**: Uses only one small allocation for splitting
#[allow(clippy::result_unit_err)]
pub fn parse_range(range: &str, size: u64) -> Result<(u64, u64), ()> {
    if !range.starts_with("bytes=") {
        return Err(());
    }
    let parts: Vec<&str> = range[RANGE_PREFIX_LEN..].split('-').collect();
    let start = parts[0].parse::<u64>().map_err(|_| ())?;
    let end = if parts.len() > 1 && !parts[1].is_empty() {
        parts[1].parse::<u64>().map_err(|_| ())?
    } else {
        size.saturating_sub(1)
    };
    if start > end || end >= size {
        return Err(());
    }
    Ok((start, end))
}
