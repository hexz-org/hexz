//! FUSE lookup, getattr, setattr, and readdir handlers.
//!
//! Implements directory operations and attribute queries for the FUSE adapter.

use super::{Strata, TTL};
use crate::vfs::InodeType;
use fuser::{ReplyAttr, ReplyDirectory, ReplyEntry, Request};
use libc::{EIO, ENOENT, EROFS};
use std::ffi::OsStr;

/// Resolves a child name under `parent` into an inode and attributes.
///
/// **Architectural intent:** Delegates name resolution to `InodeMap` and
/// surfaces overlay-consistent attributes to the kernel.
///
/// **Constraints:** Only lookups under the root inode are valid; unknown
/// names or parents produce `ENOENT`.
pub fn handle_lookup(
    fs: &mut Strata,
    _req: &Request,
    parent: u64,
    name: &OsStr,
    reply: ReplyEntry,
) {
    if let Some(inode) = fs.inodes.lookup(parent, name) {
        let attr = fs.get_merged_attr(inode);
        reply.entry(&TTL, &attr, 0);
    } else {
        reply.error(ENOENT);
    }
}

/// Returns file attributes for an inode, including overlay-backed size.
///
/// **Architectural intent:** Provides a single source of truth for FUSE
/// attribute responses that respect copy-on-write semantics.
///
/// **Constraints:** Unknown inodes are not special-cased and will return
/// attributes with size zero, leaving error handling to callers.
pub fn handle_getattr(fs: &mut Strata, _req: &Request, ino: u64, reply: ReplyAttr) {
    let attr = fs.get_merged_attr(ino);
    reply.attr(&TTL, &attr);
}

/// Applies supported attribute updates, such as truncation of the disk.
///
/// **Architectural intent:** Allows guests to resize the exported disk by
/// mutating only the overlay, keeping the base snapshot immutable.
///
/// **Constraints:** Only size changes on inode `2` are honored; all other
/// mutations and inodes return `EROFS`.
///
/// **Side effects:** Calls `set_len` on the overlay file and recomputes
/// attributes, both of which may perform blocking filesystem operations.
#[allow(clippy::too_many_arguments)]
pub fn handle_setattr(
    fs: &mut Strata,
    _req: &Request,
    ino: u64,
    _mode: Option<u32>,
    _uid: Option<u32>,
    _gid: Option<u32>,
    size: Option<u64>,
    _atime: Option<fuser::TimeOrNow>,
    _mtime: Option<fuser::TimeOrNow>,
    _ctime: Option<std::time::SystemTime>,
    _fh: Option<u64>,
    _crtime: Option<std::time::SystemTime>,
    _chgtime: Option<std::time::SystemTime>,
    _bkuptime: Option<std::time::SystemTime>,
    _flags: Option<u32>,
    reply: ReplyAttr,
) {
    if let Some(overlay) = &mut fs.overlay {
        if ino == InodeType::Disk as u64 {
            if let Some(new_size) = size {
                if overlay.file.set_len(new_size).is_err() {
                    reply.error(EIO);
                    return;
                }
            }
            let attr = fs.get_merged_attr(ino);
            reply.attr(&TTL, &attr);
            return;
        }
    }
    reply.error(EROFS);
}

/// Enumerates directory entries for the root inode.
///
/// **Architectural intent:** Presents a minimal, fixed directory layout
/// determined by `InodeMap`, typically exposing `disk` and optionally
/// `memory`.
///
/// **Constraints:** Only inode `1` is treated as a directory; all other
/// inodes result in `ENOENT`.
pub fn handle_readdir(
    fs: &mut Strata,
    _req: &Request,
    ino: u64,
    _fh: u64,
    offset: i64,
    mut reply: ReplyDirectory,
) {
    if ino != InodeType::Root as u64 {
        reply.error(ENOENT);
        return;
    }

    let entries = fs.inodes.readdir();
    for (i, entry) in entries.iter().enumerate().skip(offset as usize) {
        if reply.add(entry.inode, (i + 1) as i64, entry.kind, &entry.name) {
            break;
        }
    }
    reply.ok();
}
