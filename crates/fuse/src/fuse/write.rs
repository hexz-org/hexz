//! FUSE write handler for Strata snapshots.
//!
//! Implements copy-on-write write operations for the disk stream,
//! seeding overlay blocks with original snapshot data before modification.

use super::Strata;
use crate::vfs::BLOCK_SIZE;
use fuser::{ReplyWrite, Request};
use libc::{EIO, EROFS};
use strata_core::SnapshotStream;

/// Writes a byte range into the overlay-backed disk inode.
///
/// **Architectural intent:** Implements block-level copy-on-write by
/// first seeding each newly modified block with original snapshot bytes
/// before applying the caller's writes.
///
/// **Constraints:** Only inode `2` is writable; other inodes return
/// `EROFS`. Partial-block writes may read and rewrite full blocks.
///
/// **Side effects:** Triggers additional snapshot reads on first write to
/// each block, writes overlay data, and updates overlay metadata, which
/// can be I/O intensive for fragmented workloads.
#[allow(clippy::too_many_arguments)]
pub fn handle_write(
    fs: &mut Strata,
    _req: &Request,
    ino: u64,
    _fh: u64,
    offset: i64,
    data: &[u8],
    _write_flags: u32,
    _flags: i32,
    _lock: Option<u64>,
    reply: ReplyWrite,
) {
    if let Some(overlay) = &mut fs.overlay {
        if ino == 2 {
            let start = offset as u64;
            let len = data.len() as u64;
            let end = start + len;

            let start_block = start / BLOCK_SIZE;
            let end_block = (end - 1) / BLOCK_SIZE;

            // Seed unmodified blocks with original data
            for blk in start_block..=end_block {
                if !overlay.is_block_modified(blk) {
                    let blk_start = blk * BLOCK_SIZE;
                    if blk_start < fs.snap.size(SnapshotStream::Disk) {
                        let original = fs
                            .snap
                            .read_at(SnapshotStream::Disk, blk_start, BLOCK_SIZE as usize)
                            .unwrap_or_default();
                        if overlay.write_file(blk_start, &original).is_err() {
                            reply.error(EIO);
                            return;
                        }
                    }
                    overlay.mark_block_modified(blk);
                }
            }

            // Write the actual data
            match overlay.write_file(start, data) {
                Ok(written) => reply.written(written as u32),
                Err(e) => reply.error(e.raw_os_error().unwrap_or(EIO)),
            }
            return;
        }
    }
    reply.error(EROFS);
}
