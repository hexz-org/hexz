//! FUSE read operation.

use super::Hexz;
use fuser::ReplyData;
use libc::{EIO, ENOENT};

pub fn handle_read(fs: &mut Hexz, ino: u64, offset: i64, size: u32, reply: ReplyData) {
    let (stream, file_offset, file_size) = match fs.inodes.file_info(ino) {
        Some(info) => info,
        None => {
            reply.error(ENOENT);
            return;
        }
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
