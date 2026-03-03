//! FUSE file attribute synthesis and permission handling.

use fuser::{FileAttr, FileType};
use std::time::UNIX_EPOCH;

use super::inode::ROOT_INODE;

/// Permission bits for the root directory (owner rwx, group/other rx).
pub const PERM_DIR: u16 = 0o755;

/// Permission bits for regular files (owner rw, group/other r).
pub const PERM_FILE: u16 = 0o644;

/// Block size in bytes reported to FUSE for `stat` block counts (512).
pub const FUSE_BLOCK_SIZE: u64 = 512;

/// Synthesizes a `FileAttr` structure for a given inode and size.
pub fn make_attr(ino: u64, size: u64, uid: u32, gid: u32) -> FileAttr {
    FileAttr {
        ino,
        size,
        blocks: size.div_ceil(FUSE_BLOCK_SIZE),
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: if ino == ROOT_INODE {
            FileType::Directory
        } else {
            FileType::RegularFile
        },
        perm: if ino == ROOT_INODE {
            PERM_DIR
        } else {
            PERM_FILE
        },
        nlink: if ino == ROOT_INODE { 2 } else { 1 },
        uid,
        gid,
        rdev: 0,
        flags: 0,
        blksize: FUSE_BLOCK_SIZE as u32,
    }
}
