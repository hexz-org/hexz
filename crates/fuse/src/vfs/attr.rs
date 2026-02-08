//! File attribute synthesis for FUSE.
//!
//! Provides utilities for building `FileAttr` structures that represent
//! inodes to the FUSE kernel module, including permissions, timestamps,
//! and block counts.

use fuser::{FileAttr, FileType};
use std::time::UNIX_EPOCH;

use super::inode::InodeType;

/// Permission bits for the root directory (owner rwx, group/other rx).
///
/// Used when synthesizing `FileAttr` for the root inode. Not intended for
/// security isolation; chosen for compatibility with typical FUSE defaults.
pub const PERM_DIR: u16 = 0o755;

/// Permission bits for regular files (owner rw, group/other r).
///
/// Applied to the exported `disk` and `memory` files. Does not enforce
/// access control; callers must not rely on these for security.
pub const PERM_FILE: u16 = 0o644;

/// Block size in bytes reported to FUSE for file block counts and alignment.
///
/// Defined by the FUSE/kernel interface for `st_blocks` and `blksize`.
/// Must match the granularity used when computing block counts from file
/// size; changing it affects attribute reporting only, not overlay block size.
pub const FUSE_BLOCK_SIZE: u32 = 512;

/// Synthesizes a `FileAttr` for a given inode.
///
/// **Architectural intent:** Centralizes the logic for building FUSE attributes
/// so that both the inode map and FUSE handlers can produce consistent metadata.
///
/// **Constraints:** Uses fixed permissions and timestamps; does not implement
/// true security isolation.
pub fn make_attr(ino: u64, size: u64, uid: u32, gid: u32) -> FileAttr {
    FileAttr {
        ino,
        size,
        blocks: size.div_ceil(FUSE_BLOCK_SIZE as u64),
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: if ino == InodeType::Root as u64 {
            FileType::Directory
        } else {
            FileType::RegularFile
        },
        perm: if ino == InodeType::Root as u64 {
            PERM_DIR
        } else {
            PERM_FILE
        },
        nlink: if ino == InodeType::Root as u64 { 2 } else { 1 },
        uid,
        gid,
        rdev: 0,
        flags: 0,
        blksize: FUSE_BLOCK_SIZE,
    }
}
