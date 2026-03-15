//! FUSE read operation.

use super::Hexz;
use fuser::ReplyData;
use libc::{EIO, ENOENT};

/// Reads data from the archive for the given inode and replies via FUSE.
pub fn handle_read(fs: &Hexz, ino: u64, offset: i64, size: u32, reply: ReplyData) {
    let Some((stream, file_offset, file_size)) = fs.inodes.file_info(ino) else {
        reply.error(ENOENT);
        return;
    };

    if size == 0 {
        reply.data(&[]);
        return;
    }

    if offset < 0 {
        reply.error(libc::EINVAL);
        return;
    }

    let req_offset = offset as u64;
    if req_offset >= file_size {
        reply.data(&[]);
        return;
    }

    let actual_len = std::cmp::min(size as u64, file_size - req_offset) as usize;

    // Map virtual file offset to archive stream offset
    let absolute_offset = file_offset + req_offset;

    match fs.snap.read_at(stream, absolute_offset, actual_len) {
        Ok(data) => reply.data(&data),
        Err(_) => reply.error(EIO),
    }
}
