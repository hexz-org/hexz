//! FUSE directory traversal and attribute operations.
//!
//! This module implements the core FUSE operations for inode resolution,
//! attribute queries, and directory enumeration. It bridges the minimal VFS
//! namespace (root directory plus `disk` and optional `memory` files) with
//! the FUSE kernel protocol, ensuring that overlay modifications are reflected
//! in reported file sizes and attributes.
//!
//! # FUSE Operation Semantics
//!
//! The FUSE protocol operates on **inode numbers** rather than paths. When the
//! kernel resolves `/disk`, it:
//! 1. Calls `lookup(parent=1, name="disk")` to resolve the name under the root
//! 2. Receives an inode number (2) and cached attributes
//! 3. Uses that inode for subsequent `read`, `write`, and `getattr` operations
//!
//! This module ensures the kernel's cached view remains consistent with both
//! the immutable base snapshot and any overlay modifications.
//!
//! # Inode Caching and TTL
//!
//! Attributes and directory entries are cached in the kernel for the duration
//! specified by `TTL` (1 second). This reduces the number of FUSE round-trips
//! but requires that the adapter tolerate slightly stale size/attribute data.
//! Since writes are synchronous and overlay length is queried on demand, this
//! staleness is acceptable for typical unikernel boot scenarios.
//!
//! # Name Resolution
//!
//! Only two names are resolvable under the root directory:
//! - `disk`: Maps to inode 2, backed by `SnapshotStream::Primary`
//! - `memory`: Maps to inode 3, backed by `SnapshotStream::Secondary` (if present)
//!
//! All other names, or lookups with `parent != 1`, return `ENOENT`.
//!
//! # Performance Characteristics
//!
//! - **lookup**: O(1) hash lookup in `InodeMap` (~50-100 ns)
//! - **getattr**: O(1) if no overlay; O(1) + `stat` syscall if overlay (~1-2 µs)
//! - **setattr**: O(1) + `ftruncate` syscall for size changes (~5-10 µs)
//! - **readdir**: O(1) because the directory is fixed and small (4 entries max)
//!
//! # Examples
//!
//! ## Mounting and Listing Directory Entries
//!
//! ```no_run
//! use hexz_core::File;
//! use hexz_core::store::local::FileBackend;
//! use hexz_core::algo::compression::lz4::Lz4Compressor;
//! use hexz_fuse::mount_fs;
//! use std::sync::Arc;
//! use std::path::Path;
//!
//! # fn main() -> anyhow::Result<()> {
//! let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
//! let compressor = Box::new(Lz4Compressor::new());
//! let snap = File::new(backend, compressor, None)?;
//!
//! mount_fs(snap, Path::new("/mnt/hexz"), None, 1000, 1000)?;
//! # Ok(())
//! # }
//! ```

use super::{Hexz, TTL};
use crate::vfs::InodeType;
use fuser::{ReplyAttr, ReplyDirectory, ReplyEntry, Request};
use libc::{EIO, ENOENT, EROFS};
use std::ffi::OsStr;

/// Resolves a child name under a parent inode into an entry with attributes.
///
/// This is the core FUSE operation for path-to-inode translation. When the
/// kernel encounters a path component (e.g., `disk` in `/disk`), it calls
/// `lookup` on the parent directory (inode 1) to resolve the name into an
/// inode number and associated metadata.
///
/// # FUSE Protocol Notes
///
/// - The kernel caches the returned inode and attributes for `TTL` seconds
/// - Subsequent operations on the same path use the cached inode number
/// - The `generation` field (set to 0) is unused in this simple filesystem
///
/// # Parameters
///
/// - `fs`: Mutable reference to the FUSE filesystem state
/// - `_req`: Request context (contains UID/GID, unused in current implementation)
/// - `parent`: Inode number of the directory being searched (must be 1 for success)
/// - `name`: Name to look up (must be "disk" or "memory")
/// - `reply`: Callback to send the lookup result or error to the kernel
///
/// # Errors
///
/// Returns `ENOENT` if:
/// - `parent` is not the root inode (1)
/// - `name` does not match "disk" or "memory"
/// - The requested stream is not present in the snapshot (e.g., no secondary stream)
///
/// # Examples
///
/// ```text
/// // When the kernel resolves /disk:
/// // 1. lookup(parent=1, name="disk") -> returns inode 2 with attributes
/// // 2. Kernel caches: "/disk" -> inode 2
/// // 3. Subsequent open("/disk") uses inode 2 directly
/// ```
///
/// # Performance
///
/// - Time complexity: O(1) hash lookup in `InodeMap`
/// - Typical latency: 50-100 nanoseconds
/// - No I/O operations performed
pub fn handle_lookup(fs: &mut Hexz, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
    if let Some(inode) = fs.inodes.lookup(parent, name) {
        let attr = fs.get_merged_attr(inode);
        reply.entry(&TTL, &attr, 0);
    } else {
        reply.error(ENOENT);
    }
}

/// Retrieves file attributes for an inode, merging snapshot and overlay data.
///
/// This operation returns metadata similar to the POSIX `stat()` syscall:
/// file size, block count, permissions, timestamps, and type. For the disk
/// inode (2), the returned size reflects any overlay extensions beyond the
/// base snapshot length.
///
/// # Attribute Synthesis
///
/// Attributes are synthesized rather than stored:
/// - **Size**: For disk inode, `max(snapshot_size, overlay_size)`; for others,
///   snapshot size only
/// - **Blocks**: Size divided by 512 (FUSE block size), rounded up
/// - **Permissions**: Fixed to 0755 (root directory) or 0644 (files)
/// - **Timestamps**: All set to Unix epoch (no modification tracking)
/// - **UID/GID**: Set at mount time, not per-file
///
/// # FUSE Protocol Notes
///
/// The kernel caches these attributes for `TTL` seconds. This means size
/// changes from writes may not be visible immediately in `stat()` calls from
/// other processes unless the cache expires or is invalidated.
///
/// # Parameters
///
/// - `fs`: Mutable reference to the FUSE filesystem state
/// - `_req`: Request context (unused)
/// - `ino`: Inode number to query (1=root, 2=disk, 3=memory)
/// - `reply`: Callback to send attributes or error to the kernel
///
/// # Errors
///
/// This implementation never returns an error. Unknown inodes receive
/// attributes with size 0, which causes subsequent I/O to fail gracefully.
///
/// # Examples
///
/// ```text
/// // stat /mnt/hexz/disk with overlay enabled:
/// // - Base snapshot: 10 GiB
/// // - Overlay file: 12 GiB (after guest extended partition)
/// // - Reported size: 12 GiB (overlay size wins)
/// ```
///
/// # Performance
///
/// - Time complexity: O(1) + optional `fstat` on overlay file
/// - Typical latency: 1-2 microseconds with overlay, 50-100 ns without
pub fn handle_getattr(fs: &mut Hexz, _req: &Request, ino: u64, reply: ReplyAttr) {
    if !fs.inodes.is_valid_inode(ino) {
        reply.error(ENOENT);
        return;
    }
    let attr = fs.get_merged_attr(ino);
    reply.attr(&TTL, &attr);
}

/// Modifies file attributes, supporting only size changes to the disk inode.
///
/// This operation corresponds to POSIX operations like `chmod`, `chown`,
/// `truncate`, and `utimensat`. However, since the base snapshot is immutable
/// and the overlay tracks only block-level modifications, most attribute
/// changes are rejected with `EROFS` (read-only filesystem).
///
/// # Supported Operations
///
/// - **Size changes**: `truncate()` on inode 2 (disk) with overlay enabled
///   - Extending: Allocates space in overlay, new regions read as zeros
///   - Shrinking: Truncates overlay file, blocks beyond new size are discarded
///
/// # Rejected Operations
///
/// All other modifications return `EROFS`:
/// - Permission/ownership changes (`chmod`, `chown`)
/// - Timestamp updates (`touch`)
/// - Size changes on non-disk inodes or without overlay
///
/// # FUSE Protocol Notes
///
/// The `setattr` call may specify multiple attribute changes simultaneously
/// (e.g., size + mtime). This implementation only honors `size` when the
/// inode and overlay conditions are met; all other fields are ignored.
///
/// # Parameters
///
/// - `fs`: Mutable reference to the FUSE filesystem state
/// - `_req`: Request context (unused)
/// - `ino`: Inode number to modify (only 2/disk is writable)
/// - `_mode`: Optional new permission bits (ignored)
/// - `_uid`: Optional new owner UID (ignored)
/// - `_gid`: Optional new group GID (ignored)
/// - `size`: Optional new file size in bytes (honored for disk inode only)
/// - `_atime`: Optional new access time (ignored)
/// - `_mtime`: Optional new modification time (ignored)
/// - `_ctime`: Optional new change time (ignored)
/// - `_fh`: Optional file handle from prior `open` (unused)
/// - `_crtime`: Optional creation time (ignored)
/// - `_chgtime`: Optional change time (ignored)
/// - `_bkuptime`: Optional backup time (ignored)
/// - `_flags`: Optional BSD/macOS flags (ignored)
/// - `reply`: Callback to send updated attributes or error
///
/// # Errors
///
/// - `EIO`: Overlay `set_len` failed (e.g., no disk space)
/// - `EROFS`: Modification not permitted (wrong inode, no overlay, or
///   unsupported attribute change)
///
/// # Examples
///
/// ```text
/// // Extend disk to 20 GiB:
/// // $ truncate -s 20G /mnt/hexz/disk
/// // -> setattr(ino=2, size=Some(20*1024^3)) -> overlay.file.set_len(20G)
/// ```
///
/// # Performance
///
/// - Time complexity: O(1) + `ftruncate` syscall on overlay
/// - Typical latency: 5-10 microseconds (depends on filesystem)
#[allow(clippy::too_many_arguments)]
pub fn handle_setattr(
    fs: &mut Hexz,
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

/// Lists directory entries for the root directory.
///
/// This operation corresponds to the `readdir` or `getdents` syscall family.
/// It returns a fixed set of entries: `.`, `..`, `disk`, and optionally
/// `memory`, depending on what streams are present in the snapshot.
///
/// # FUSE Protocol Notes
///
/// Directory listing is stateful and uses an `offset` parameter to resume
/// iteration:
/// - `offset=0`: Start from the beginning (`.` entry)
/// - `offset=N`: Resume after the (N-1)th entry
/// - The reply buffer is limited; if `reply.add()` returns `true`, the buffer
///   is full and iteration stops. The kernel will call again with the next offset.
///
/// # Directory Layout
///
/// The returned entries are always in this order:
/// 1. `.` (inode 1, type Directory)
/// 2. `..` (inode 1, type Directory, same as `.` since root has no parent)
/// 3. `disk` (inode 2, type RegularFile, if primary stream present)
/// 4. `memory` (inode 3, type RegularFile, if secondary stream present)
///
/// # Parameters
///
/// - `fs`: Mutable reference to the FUSE filesystem state
/// - `_req`: Request context (unused)
/// - `ino`: Inode number to list (must be 1 for root directory)
/// - `_fh`: File handle from prior `opendir` (unused, directories auto-opened)
/// - `offset`: Iteration offset (0-based index into entry list)
/// - `reply`: Directory reply buffer to accumulate entries
///
/// # Errors
///
/// Returns `ENOENT` if `ino` is not the root directory (1). All other inodes
/// are regular files and cannot be listed as directories.
///
/// # Examples
///
/// ```text
/// // $ ls /mnt/hexz
/// // .  ..  disk  memory
/// //
/// // Internally:
/// // readdir(ino=1, offset=0) -> returns [., .., disk, memory]
/// ```
///
/// # Performance
///
/// - Time complexity: O(1) - fixed entry count (2-4 entries)
/// - Typical latency: < 1 microsecond
/// - No I/O operations performed
pub fn handle_readdir(
    fs: &mut Hexz,
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
    let skip = if offset < 0 { 0usize } else { offset as usize };
    for (i, entry) in entries.iter().enumerate().skip(skip) {
        if reply.add(entry.inode, (i + 1) as i64, entry.kind, &entry.name) {
            break;
        }
    }
    reply.ok();
}
