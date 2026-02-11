//! FUSE read handler for Strata snapshots.
//!
//! Implements read operations with overlay support, merging base snapshot
//! data with copy-on-write blocks.

use super::Strata;
use crate::vfs::BLOCK_SIZE;
use fuser::{ReplyData, Request};
use libc::{EIO, ENOENT};
use strata_core::SnapshotStream;

/// Reads a byte range from a file, honoring overlay modifications.
///
/// **Architectural intent:** Merges base snapshot data with overlay
/// blocks so callers observe a single coherent disk image. This enables
/// copy-on-write (COW) semantics where modified blocks override base data.
///
/// **Constraints:** Overlay reads only apply to the disk stream; when no
/// overlay is present or when reading the memory stream, data is served
/// directly from the snapshot.
///
/// **Performance characteristics:**
/// - **Fast path** (no overlay or memory stream): Single read from snapshot (~1-5 µs)
/// - **Overlay path**: Iterates over affected 4 KiB blocks, reading from overlay or
///   snapshot per block (~5-20 µs depending on request size and overlay coverage)
/// - **Memory allocation**: Allocates a buffer of `size` bytes for the response
///
/// **Concurrency:** Multiple concurrent reads are safe because:
/// - Snapshot reads are immutable and thread-safe
/// - Overlay reads use interior mutability with proper locking
///
/// **Side effects:** Allocates temporary buffers and may perform multiple
/// reads to both snapshot backend and overlay file per call.
#[allow(clippy::too_many_arguments)]
pub fn handle_read(
    fs: &mut Strata,
    _req: &Request,
    ino: u64,
    _fh: u64,
    offset: i64,
    size: u32,
    _flags: i32,
    _lock: Option<u64>,
    reply: ReplyData,
) {
    let stream = match fs.inodes.inode_to_stream(ino) {
        Some(s) => s,
        None => {
            reply.error(ENOENT);
            return;
        }
    };

    // Fast path: no overlay or reading memory stream
    if fs.overlay.is_none() || stream != SnapshotStream::Disk {
        match fs.snap.read_at(stream, offset as u64, size as usize) {
            Ok(data) => reply.data(&data),
            Err(_) => reply.error(EIO),
        }
        return;
    }

    // Overlay merge path
    let mut buffer = vec![0u8; size as usize];
    let start = offset as u64;
    let end = start + size as u64;

    let start_block = start / BLOCK_SIZE;
    let end_block = (end - 1) / BLOCK_SIZE;

    for blk in start_block..=end_block {
        let blk_start = blk * BLOCK_SIZE;
        let blk_end = blk_start + BLOCK_SIZE;

        let req_start = std::cmp::max(start, blk_start);
        let req_end = std::cmp::min(end, blk_end);

        if req_start >= req_end {
            continue;
        }

        let len = (req_end - req_start) as usize;
        let buf_offset = (req_start - start) as usize;

        let is_modified = fs.overlay.as_ref().unwrap().is_block_modified(blk);

        if is_modified {
            if let Some(ov) = &mut fs.overlay {
                let mut temp = vec![0u8; len];
                if ov.read_file(req_start, &mut temp).is_ok() {
                    buffer[buf_offset..buf_offset + len].copy_from_slice(&temp);
                }
            }
        } else {
            let snap_size = fs.snap.size(stream);
            if req_start < snap_size {
                if let Ok(data) = fs.snap.read_at(stream, req_start, len) {
                    let copy_len = std::cmp::min(data.len(), len);
                    buffer[buf_offset..buf_offset + copy_len].copy_from_slice(&data[..copy_len]);
                }
            }
        }
    }

    reply.data(&buffer);
}
