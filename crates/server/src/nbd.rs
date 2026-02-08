use anyhow::Result;
use std::sync::Arc;
use strata_core::{SnapshotStream, StrataFile};
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

pub async fn handle_client(mut socket: TcpStream, snap: Arc<StrataFile>) -> Result<()> {
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
