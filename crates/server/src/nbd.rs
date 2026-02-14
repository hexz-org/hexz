//! Network Block Device (NBD) protocol server implementation.
//!
//! This module implements the NBD protocol (version 3.0+, fixed newstyle negotiation)
//! to expose Hexz snapshots as block devices over TCP. Clients can mount the snapshot
//! using standard NBD client tools like `nbd-client` (Linux) or connect directly via
//! the NBD protocol.
//!
//! # Protocol Overview
//!
//! The NBD protocol consists of three phases:
//!
//! 1. **Handshake**: Server announces capabilities (flags) and magic values
//! 2. **Option Negotiation**: Client requests export info and flags
//! 3. **Transmission**: Client sends read/write/flush/trim commands
//!
//! This implementation follows the "fixed newstyle" negotiation introduced in NBD 3.0,
//! which is more robust than the legacy "oldstyle" protocol.
//!
//! # Protocol Reference
//!
//! - NBD Protocol Specification: <https://github.com/NetworkBlockDevice/nbd/blob/master/doc/proto.md>
//! - RFC (draft): <https://www.ietf.org/archive/id/draft-ietf-nbd-protocol-00.html>
//!
//! # Security Considerations
//!
//! - **Read-only mode**: This implementation always exports snapshots as read-only
//!   to prevent accidental modification
//! - **No encryption**: The NBD protocol does not include built-in encryption.
//!   For secure access over untrusted networks, use an SSH tunnel or VPN.
//! - **No authentication**: NBD does not provide authentication. Access control
//!   must be implemented at the network level (firewall, localhost-only binding).
//!
//! # Performance Characteristics
//!
//! - **Throughput**: Typically limited by snapshot decompression (~500-2000 MB/s)
//!   rather than network bandwidth for local connections
//! - **Latency**: Read latency includes network RTT + decompression time (~1-5 ms total)
//! - **Concurrency**: Each client connection is handled by a separate Tokio task
//!
//! # Example Usage
//!
//! ```no_run
//! # use std::sync::Arc;
//! # use hexz_core::File;
//! # use hexz_server::nbd::handle_client;
//! # use tokio::net::TcpListener;
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! // Server-side (in hexz-server)
//! let listener = TcpListener::bind("127.0.0.1:10809").await?;
//! // ... load snapshot into Arc<File> ...
//! # let snap: Arc<File> = Arc::new(todo!());
//!
//! loop {
//!     let (socket, _) = listener.accept().await?;
//!     let snap = snap.clone();
//!     tokio::spawn(async move {
//!         if let Err(e) = handle_client(socket, snap).await {
//!             eprintln!("NBD client error: {}", e);
//!         }
//!     });
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Client-Side Usage (Linux)
//!
//! ```bash
//! # Connect NBD client to server
//! sudo nbd-client localhost 10809 /dev/nbd0
//!
//! # Mount the block device
//! sudo mount -o ro /dev/nbd0 /mnt/snapshot
//!
//! # Disconnect when done
//! sudo nbd-client -d /dev/nbd0
//! ```

use anyhow::Result;
use hexz_core::{File, SnapshotStream};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const NBD_MAGIC: u64 = 0x4e42444d41474943;
const NBD_OPT_MAGIC: u64 = 0x49484156454F5054;
const NBD_REP_MAGIC: u64 = 0x3e889045565a9;

const NBD_FLAG_FIXED_NEWSTYLE: u16 = 1 << 0;
const NBD_FLAG_NO_ZEROES: u16 = 1 << 1;

const NBD_FLAG_HAS_FLAGS: u16 = 1 << 0;
const NBD_FLAG_READ_ONLY: u16 = 1 << 1;

const NBD_OPT_EXPORT_NAME: u32 = 1;
const NBD_OPT_ABORT: u32 = 2;
const NBD_OPT_INFO: u32 = 6;
const NBD_OPT_GO: u32 = 7;

const NBD_REP_ACK: u32 = 1;
const NBD_REP_INFO: u32 = 3;

const NBD_INFO_EXPORT: u16 = 0;

const NBD_CMD_READ: u16 = 0;
const NBD_CMD_WRITE: u16 = 1;
const NBD_CMD_DISC: u16 = 2;
const NBD_CMD_FLUSH: u16 = 3;
const NBD_CMD_TRIM: u16 = 4;

const NBD_REQUEST_MAGIC: u32 = 0x25609513;
const NBD_REPLY_MAGIC: u32 = 0x67446698;

/// Handle a single NBD client connection.
///
/// This function implements the complete NBD server lifecycle for one client:
/// 1. Performs the NBD handshake and option negotiation
/// 2. Enters the transmission phase to serve read requests
/// 3. Handles disconnect when the client sends NBD_CMD_DISC
///
/// The connection is read-only and blocks are served directly from the snapshot's
/// disk stream. Write, flush, and trim commands return error responses.
///
/// # Connection Lifecycle
///
/// ```text
/// Client connects → Handshake → Option Negotiation → Transmission → Disconnect
///                      ↓              ↓                    ↓
///                  Send magic    Send export info    Serve read commands
/// ```
///
/// # Arguments
///
/// - `socket`: TCP connection to the NBD client
/// - `snap`: Shared reference to the Hexz snapshot file
///
/// # Returns
///
/// Returns `Ok(())` when the client disconnects normally, or an error if the protocol
/// is violated or I/O fails.
///
/// # Errors
///
/// This function returns an error if:
/// - The client sends invalid magic values or malformed requests
/// - Socket I/O fails (connection reset, timeout, etc.)
/// - The snapshot cannot be read (decompression errors, backend failures)
pub async fn handle_client(mut socket: TcpStream, snap: Arc<File>) -> Result<()> {
    // --- Handshake (Fixed Newstyle) ---

    // 1. Send Init Pass
    socket.write_u64(NBD_MAGIC).await?;
    socket.write_u64(NBD_OPT_MAGIC).await?;
    // Global flags: FIXED_NEWSTYLE | NO_ZEROES
    socket
        .write_u16(NBD_FLAG_FIXED_NEWSTYLE | NBD_FLAG_NO_ZEROES)
        .await?;

    // 2. Receive Client Flags
    let client_flags = socket.read_u32().await?;
    let _supports_no_zeroes = (client_flags & (NBD_FLAG_NO_ZEROES as u32)) != 0;

    // 3. Option Negotiation Loop
    loop {
        let magic = socket.read_u64().await?;
        if magic != NBD_OPT_MAGIC {
            anyhow::bail!("Invalid option magic");
        }

        let opt_id = socket.read_u32().await?;
        let opt_len = socket.read_u32().await?;

        // Read option data
        let mut opt_data = vec![0u8; opt_len as usize];
        socket.read_exact(&mut opt_data).await?;

        match opt_id {
            NBD_OPT_ABORT => return Ok(()),
            NBD_OPT_EXPORT_NAME => {
                // Old-style negotiation finish.
                let size = snap.size(SnapshotStream::Disk);
                let export_flags = NBD_FLAG_HAS_FLAGS | NBD_FLAG_READ_ONLY;

                socket.write_u64(size).await?;
                socket.write_u16(export_flags).await?;
                // NO_ZEROES means we don't send 124 bytes of zeroes here.
                break;
            }
            NBD_OPT_INFO | NBD_OPT_GO => {
                let size = snap.size(SnapshotStream::Disk);
                let export_flags = NBD_FLAG_HAS_FLAGS | NBD_FLAG_READ_ONLY;

                // Reply NBD_INFO_EXPORT
                socket.write_u64(NBD_REP_MAGIC).await?;
                socket.write_u32(opt_id).await?;
                socket.write_u32(NBD_REP_INFO).await?;
                socket.write_u32(12).await?; // Length of info block
                socket.write_u16(NBD_INFO_EXPORT).await?;
                socket.write_u64(size).await?;
                socket.write_u16(export_flags).await?;

                // Reply ACK
                socket.write_u64(NBD_REP_MAGIC).await?;
                socket.write_u32(opt_id).await?;
                socket.write_u32(NBD_REP_ACK).await?;
                socket.write_u32(0).await?;

                if opt_id == NBD_OPT_GO {
                    break;
                }
            }
            _ => {
                // Unsupported option: Reply ERR_UNSUP (2^31 + 1)
                socket.write_u64(NBD_REP_MAGIC).await?;
                socket.write_u32(opt_id).await?;
                socket.write_u32(2147483649 + 1).await?;
                socket.write_u32(0).await?;
            }
        }
    }

    // --- Transmission Phase ---

    loop {
        let magic = socket.read_u32().await?;
        if magic != NBD_REQUEST_MAGIC {
            anyhow::bail!("Invalid request magic: {:x}", magic);
        }

        let _flags = socket.read_u16().await?;
        let type_ = socket.read_u16().await?;
        let handle = socket.read_u64().await?;
        let offset = socket.read_u64().await?;
        let length = socket.read_u32().await?;

        match type_ {
            NBD_CMD_READ => {
                let mut error = 0u32;
                let data = match snap.read_at(SnapshotStream::Disk, offset, length as usize) {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::error!("Read error: {}", e);
                        error = 5; // EIO
                        Vec::new()
                    }
                };

                // Reply header
                socket.write_u32(NBD_REPLY_MAGIC).await?;
                socket.write_u32(error).await?;
                socket.write_u64(handle).await?;

                // Payload
                if error == 0 {
                    socket.write_all(&data).await?;
                    // If read was short (EOF), pad with zeros
                    if data.len() < length as usize {
                        let padding = vec![0u8; length as usize - data.len()];
                        socket.write_all(&padding).await?;
                    }
                }
            }
            NBD_CMD_DISC => {
                return Ok(());
            }
            NBD_CMD_WRITE | NBD_CMD_TRIM | NBD_CMD_FLUSH => {
                // We are read-only. Read payload if write to drain socket
                if type_ == NBD_CMD_WRITE {
                    let mut buf = vec![0u8; length as usize];
                    socket.read_exact(&mut buf).await?;
                }

                // Return EPERM (1)
                let error = 1u32;
                socket.write_u32(NBD_REPLY_MAGIC).await?;
                socket.write_u32(error).await?;
                socket.write_u64(handle).await?;
            }
            _ => {
                // Unknown command: EINVAL (22)
                let error = 22u32;
                socket.write_u32(NBD_REPLY_MAGIC).await?;
                socket.write_u32(error).await?;
                socket.write_u64(handle).await?;
            }
        }
    }
}
