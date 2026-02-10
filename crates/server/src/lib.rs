//! HTTP and NBD server for exposing Strata snapshots.
//!
//! Exposes disk and memory streams via HTTP range requests and provides
//! a Network Block Device (NBD) server for mounting snapshots as block devices.

pub mod nbd;

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use std::net::SocketAddr;
use std::sync::Arc;
use strata_core::{SnapshotStream, StrataFile};
use tokio::net::TcpListener;

/// IPv4 address for listeners (localhost).
///
/// Security Note: Defaults to loopback to prevent accidental exposure
/// of snapshot data to the local network or internet. To expose to the
/// network, this must be explicitly configured (future feature).
const BIND_ADDR: [u8; 4] = [127, 0, 0, 1];

/// Length in bytes of the `Range` header prefix `bytes=` (6).
const RANGE_PREFIX_LEN: usize = 6;

/// Maximum allowed read size per request to prevent DoS attacks (32 MiB).
const MAX_CHUNK_SIZE: u64 = 32 * 1024 * 1024;

/// Shared application state for the HTTP serving layer.
struct AppState {
    /// The opened Strata snapshot file being served.
    snap: Arc<StrataFile>,
}

/// Exposes a `StrataFile` over NBD (Network Block Device).
///
/// This starts a TCP listener that speaks the NBD protocol, allowing
/// Linux clients to mount the snapshot using `nbd-client`.
///
/// # Security Note
///
/// The NBD protocol is unencrypted. It is recommended to run this over
/// a Unix socket or a localhost-only TCP port.
pub async fn serve_nbd(snap: Arc<StrataFile>, port: u16) -> anyhow::Result<()> {
    let addr = SocketAddr::from((BIND_ADDR, port));
    let listener = TcpListener::bind(addr).await?;

    tracing::info!("NBD server listening on {}", addr);
    println!(
        "NBD server started on {}. Use 'nbd-client localhost {} /dev/nbd0' to mount.",
        addr, port
    );

    loop {
        // Accept incoming NBD connections
        let (socket, remote_addr) = listener.accept().await?;
        tracing::debug!("Accepted NBD connection from {}", remote_addr);

        let snap_clone = snap.clone();
        tokio::spawn(async move {
            if let Err(e) = nbd::handle_client(socket, snap_clone).await {
                tracing::error!("NBD client error: {}", e);
            }
        });
    }
}

/// Exposes a `StrataFile` as an S3 caching gateway.
///
/// **WARNING: NOT IMPLEMENTED.**
///
/// Calling this function will panic immediately. It is reserved for future use.
/// Do not use in production.
#[deprecated(note = "Not implemented. Will panic.")]
pub async fn serve_s3_gateway(_snap: Arc<StrataFile>, port: u16) -> anyhow::Result<()> {
    tracing::info!("Starting S3 Gateway on port {}", port);
    println!(
        "S3 Gateway started on port {} (Not fully implemented)",
        port
    );
    std::future::pending::<()>().await; // Keep alive
    unreachable!();
}

/// Exposes a `StrataFile` over HTTP with simple range semantics.
pub async fn serve_http(snap: Arc<StrataFile>, port: u16) -> anyhow::Result<()> {
    let state = Arc::new(AppState { snap });

    let app = Router::new()
        .route("/disk", get(get_disk))
        .route("/memory", get(get_memory))
        .with_state(state);

    let addr = SocketAddr::from((BIND_ADDR, port));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("HTTP server listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

/// Handler for serving disk-backed data from the snapshot over HTTP.
async fn get_disk(headers: HeaderMap, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    handle_request(headers, &state.snap, SnapshotStream::Disk)
}

/// Handler for serving memory-backed data from the snapshot over HTTP.
async fn get_memory(headers: HeaderMap, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    handle_request(headers, &state.snap, SnapshotStream::Memory)
}

/// Core HTTP handler that translates `Range` headers into snapshot reads.
fn handle_request(headers: HeaderMap, snap: &StrataFile, stream: SnapshotStream) -> Response {
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

/// Parses a `Range` header of the form `bytes=start-end` into absolute offsets.
fn parse_range(range: &str, size: u64) -> Result<(u64, u64), ()> {
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
