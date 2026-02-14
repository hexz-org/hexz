//! HTTP server for exposing Hexz snapshots over network protocols.
//!
//! This command starts a server that exposes Hexz snapshot data over various
//! network protocols (HTTP, NBD, S3), enabling remote access without local
//! snapshot files. Supports daemon mode for background operation and is designed
//! for high-performance remote snapshot access.
//!
//! # Server Modes
//!
//! ## HTTP Mode (Default)
//!
//! **Protocol:** HTTP/1.1 with Range request support
//! **Port:** Configurable (default: varies by invocation)
//! **Endpoints:**
//! - `GET /disk` - Serves disk image stream
//! - `GET /memory` - Serves memory dump stream (if present)
//! - `GET /info` - Returns snapshot metadata (JSON)
//!
//! **Range Request Support:**
//! - Supports `Range: bytes=start-end` header
//! - Enables efficient random access and partial reads
//! - Used by HTTP storage backend for transparent remote mounting
//!
//! **Use Cases:**
//! - Remote snapshot access over networks
//! - HTTP storage backend testing
//! - Web-based snapshot browsing
//! - Container registry-like snapshot distribution
//!
//! **Performance:**
//! - Sequential reads: ~100-500 MB/s (network-bound)
//! - Random reads: ~1000-5000 IOPS (depends on network latency)
//! - Concurrent connections: Handled via Tokio async runtime
//!
//! ## NBD Mode (`--nbd`)
//!
//! **Protocol:** NBD (Network Block Device) protocol
//! **Port:** Configurable
//! **Features:**
//! - Block device semantics (read-only currently)
//! - Compatible with standard NBD clients (`nbd-client`)
//! - Lower overhead than HTTP for sequential access
//!
//! **Use Cases:**
//! - Remote block device mounting
//! - Performance testing and benchmarking
//! - Integration with NBD-aware tools
//!
//! **Client Usage:**
//! ```bash
//! # Start NBD server
//! hexz serve snapshot.st --nbd --port 10809
//!
//! # Connect from client
//! sudo nbd-client server-ip 10809 /dev/nbd0
//! sudo mount /dev/nbd0 /mnt
//! ```
//!
//! ## S3 Gateway Mode (`--s3`)
//!
//! **Protocol:** S3-compatible REST API
//! **Status:** Not yet implemented
//! **Planned Features:**
//! - S3 GetObject with range support
//! - Bucket/object listing
//! - Compatible with S3 storage backend
//!
//! # Server Configuration
//!
//! **Port Binding:**
//! - Binds to `127.0.0.1:<port>` (localhost only)
//! - Default ports vary by mode (consult CLI help)
//! - Ensure firewall rules allow inbound connections
//!
//! **Daemon Mode:**
//! - Detaches from terminal and runs in background
//! - Logs redirected to `/tmp/hexz-serve.log` and `/tmp/hexz-serve.err`
//! - Working directory: Current directory (not `/`)
//! - No PID file created (use systemd or similar for management)
//!
//! **Snapshot Loading:**
//! - Opens snapshot read-only
//! - Loads compression dictionary if present
//! - Initializes appropriate decompressor (LZ4 or Zstd)
//! - No caching configured (each request decompresses on-demand)
//!
//! # Security Considerations
//!
//! **WARNING:** This server has no authentication or encryption.
//!
//! - **Do not expose to untrusted networks**
//! - Use behind a reverse proxy (nginx, haproxy) for production
//! - Consider TLS termination at proxy level
//! - Implement authentication at proxy layer
//!
//! **Recommendations:**
//! - Bind to localhost (`127.0.0.1`) for local access
//! - Use SSH tunneling for remote access: `ssh -L 8080:localhost:8080 server`
//! - Deploy behind VPN for internal network access
//! - Use firewall rules to restrict access
//!
//! # Performance Tuning
//!
//! **Concurrency:**
//! - Uses Tokio multi-threaded runtime (all CPU cores)
//! - Each connection handled concurrently
//! - No per-connection memory overhead beyond request buffers
//!
//! **Compression:**
//! - Decompression happens on-demand for each request
//! - No server-side caching (stateless design)
//! - LZ4: ~800-2000 MB/s decompression per core
//! - Zstd: ~400-800 MB/s decompression per core
//!
//! **Network:**
//! - Performance limited by network bandwidth and latency
//! - Use 10 GbE or faster for multi-GB snapshots
//! - Consider proximity to clients (same datacenter/region)
//!
//! # Common Usage Patterns
//!
//! ```bash
//! # Start HTTP server on port 8080
//! hexz serve snapshot.st --port 8080
//!
//! # Start as daemon (background process)
//! hexz serve snapshot.st --port 8080 --daemon
//!
//! # Start NBD server
//! hexz serve snapshot.st --nbd --port 10809
//!
//! # Access from remote client
//! curl -H "Range: bytes=0-1024" http://server:8080/disk
//!
//! # Mount via HTTP backend
//! hexz mount http://server:8080 /mnt
//! ```

use anyhow::Result;
use daemonize::Daemonize;
use hexz_core::File as HexzFile;
use hexz_core::store::local::FileBackend;
use std::fs::File;
use std::sync::Arc;

/// Executes the serve command to start a network server.
///
/// Opens a Hexz snapshot and starts a server that exposes it over HTTP, NBD,
/// or S3 protocol. The server runs until interrupted (Ctrl+C) or, in daemon mode,
/// until explicitly killed.
///
/// # Arguments
///
/// * `hexz_path` - Path to the `.st` snapshot file to serve
/// * `port` - TCP port to bind to
/// * `daemon` - If true, daemonize the process and run in background
/// * `nbd` - If true, use NBD protocol; otherwise use HTTP
/// * `s3` - If true, use S3 gateway mode (not yet implemented)
///
/// # Server Startup Sequence
///
/// 1. **Daemonization** (if requested):
///    - Detach from terminal
///    - Redirect stdout to `/tmp/hexz-serve.log`
///    - Redirect stderr to `/tmp/hexz-serve.err`
///
/// 2. **Snapshot Loading**:
///    - Open snapshot file via `FileBackend`
///    - Read and parse header
///    - Load compression dictionary if present
///    - Initialize decompressor (LZ4 or Zstd)
///
/// 3. **Server Start**:
///    - Create Tokio multi-threaded runtime
///    - Start HTTP or NBD server on specified port
///    - Listen for incoming connections
///
/// 4. **Serving**:
///    - Handle requests until process is interrupted
///    - Each request decompresses data on-demand
///    - No persistent state or caching
///
/// # Protocol Selection
///
/// - If `nbd=true`: Start NBD server via `hexz_server::serve_nbd()`
/// - If `s3=true`: Reserved for future S3 gateway (currently prints error)
/// - Otherwise: Start HTTP server via `hexz_server::serve_http()`
///
/// # Errors
///
/// Returns an error if:
/// - Snapshot file cannot be opened or read
/// - Header or dictionary cannot be parsed (corrupted snapshot)
/// - Port is already in use (address binding fails)
/// - Daemonization fails (resource limits, permissions)
/// - Server fails to start (Tokio runtime error)
///
/// # Examples
///
/// ```no_run
/// use hexz_cli::cmd::sys::serve;
///
/// // Start HTTP server on port 8080
/// serve::run(
///     "snapshot.hxz".to_string(),
///     8080,
///     false, // not daemon
///     false, // HTTP mode
///     false, // not S3
/// )?;
///
/// // Start NBD server as daemon
/// serve::run(
///     "snapshot.hxz".to_string(),
///     10809,
///     true,  // daemon mode
///     true,  // NBD mode
///     false,
/// )?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run(hexz_path: String, port: u16, daemon: bool, nbd: bool, s3: bool) -> Result<()> {
    if daemon {
        let log_dir = std::env::var("XDG_RUNTIME_DIR")
            .or_else(|_| std::env::var("TMPDIR"))
            .unwrap_or_else(|_| "/tmp".to_string());
        let stdout = File::create(format!("{}/hexz-serve.log", log_dir))
            .or_else(|_| File::create("/dev/null"))?;
        let stderr = File::create(format!("{}/hexz-serve.err", log_dir))
            .or_else(|_| File::create("/dev/null"))?;

        Daemonize::new()
            .working_directory(".")
            .stdout(stdout)
            .stderr(stderr)
            .start()?;
    } else {
        println!("Starting Hexz server on port {}", port);
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let backend = Arc::new(FileBackend::new(std::path::Path::new(&hexz_path))?);
            let snap = HexzFile::open(backend, None)?;

            if nbd {
                hexz_server::serve_nbd(snap, port).await
            } else if s3 {
                eprintln!("Error: S3 gateway feature is not yet implemented.");
                Ok(())
            } else {
                hexz_server::serve_http(snap, port).await
            }
        })
}
