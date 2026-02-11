//! FUSE file attribute synthesis and permission handling.
//!
//! This module provides utilities for constructing `FileAttr` structures that
//! represent inodes to the FUSE kernel. Since the Strata filesystem is
//! minimal and snapshot-based, attributes are **synthesized** rather than
//! stored on disk. This module centralizes the logic for computing permissions,
//! timestamps, block counts, and file types from inode numbers and logical sizes.
//!
//! # Attribute Synthesis Strategy
//!
//! Unlike traditional filesystems that store metadata in inodes on disk,
//! Strata generates attributes on demand:
//! - **Permissions**: Fixed values (0755 for root, 0644 for files) for simplicity
//! - **Timestamps**: All set to Unix epoch (no modification tracking needed)
//! - **UID/GID**: Set at mount time and applied uniformly to all inodes
//! - **Size**: Derived from snapshot stream lengths and overlay file size
//! - **Block count**: Computed from size using FUSE block size (512 bytes)
//!
//! This approach eliminates the need for persistent metadata storage and
//! simplifies the FUSE adapter at the cost of not tracking true modification
//! times or per-file permissions.
//!
//! # Permission Handling
//!
//! Permissions are **not enforced by the FUSE layer**. The kernel's
//! `DefaultPermissions` mount option delegates permission checks to the
//! kernel's VFS layer, which uses the synthesized permissions. However, since
//! all inodes have the same owner and permissions are fixed, this is primarily
//! for compatibility with tools that inspect file modes (e.g., `ls -l`).
//!
//! For security isolation (e.g., preventing non-root users from accessing the
//! snapshot), rely on filesystem-level permissions on the mount point and
//! snapshot file, not on synthesized inode permissions.
//!
//! # Timestamp Handling
//!
//! All timestamps (atime, mtime, ctime, crtime) are set to the Unix epoch
//! (1970-01-01 00:00:00 UTC). This reflects the immutable nature of snapshots:
//! - The base snapshot never changes, so modification time is irrelevant
//! - Overlay writes do not update timestamps (could be added if needed)
//! - Access time tracking is disabled for performance
//!
//! Tools that rely on modification times (e.g., `make`, `rsync`) will see
//! epoch timestamps and may behave unexpectedly.
//!
//! # Block Count Computation
//!
//! The `blocks` field in `FileAttr` represents the number of 512-byte blocks
//! allocated to the file, as reported by `stat()`. This is computed as:
//! ```text
//! blocks = size.div_ceil(FUSE_BLOCK_SIZE)
//! ```
//! where `FUSE_BLOCK_SIZE = 512` (the standard `stat` block size).
//!
//! For overlay-extended files, the block count reflects the overlay size, not
//! the base snapshot size. This ensures that tools like `du` report accurate
//! space usage.
//!
//! # Examples
//!
//! ## Synthesizing Root Directory Attributes
//!
//! ```
//! use strata_fuse::vfs::attr::make_attr;
//!
//! let attr = make_attr(1, 0, 1000, 1000);
//! assert_eq!(attr.kind, fuser::FileType::Directory);
//! assert_eq!(attr.perm, 0o755);
//! assert_eq!(attr.size, 0); // Directories have zero size in FUSE
//! ```
//!
//! ## Synthesizing Disk File Attributes
//!
//! ```
//! use strata_fuse::vfs::attr::make_attr;
//!
//! let attr = make_attr(2, 10 * 1024 * 1024 * 1024, 1000, 1000); // 10 GiB disk
//! assert_eq!(attr.kind, fuser::FileType::RegularFile);
//! assert_eq!(attr.perm, 0o644);
//! assert_eq!(attr.blocks, (10 * 1024 * 1024 * 1024) / 512);
//! ```

use fuser::{FileAttr, FileType};
use std::time::UNIX_EPOCH;

use super::inode::InodeType;

/// Permission bits for the root directory (owner rwx, group/other rx).
///
/// Set to 0755 (rwxr-xr-x) to allow owner full access and others read/execute.
/// This matches typical directory permissions and allows `ls` and `cd` to
/// work as expected.
///
/// # Security Note
///
/// This permission is synthesized and not enforced by the FUSE layer. Actual
/// access control is delegated to the kernel's VFS via `DefaultPermissions`
/// mount option. For true isolation, control access at the mount point level.
pub const PERM_DIR: u16 = 0o755;

/// Permission bits for regular files (owner rw, group/other r).
///
/// Set to 0644 (rw-r--r--) to allow owner read/write and others read-only.
/// Applied to both `disk` and `memory` files. When an overlay is enabled,
/// the owner can write to `disk`; otherwise, writes fail with `EROFS`.
///
/// # Note
///
/// These permissions are not enforced by the FUSE layer. The actual
/// writability of `disk` is controlled by overlay presence, not by this
/// permission mask.
pub const PERM_FILE: u16 = 0o644;

/// Block size in bytes reported to FUSE for `stat` block counts (512).
///
/// This is the standard block size used by POSIX `stat()` for the `st_blocks`
/// field. It represents the number of 512-byte blocks allocated to a file,
/// not the actual filesystem or overlay block size (which is 4096 bytes).
///
/// # Relation to Overlay Block Size
///
/// Do not confuse this with the overlay's `BLOCK_SIZE` (4 KiB), which controls
/// the granularity of copy-on-write tracking. `FUSE_BLOCK_SIZE` is purely for
/// attribute reporting and matches the kernel's expectation for `st_blksize`.
///
/// # Block Count Calculation
///
/// The `blocks` field in `FileAttr` is computed as:
/// ```text
/// blocks = file_size.div_ceil(512)
/// ```
/// This ensures that tools like `du` and `ls -s` report reasonable values.
pub const FUSE_BLOCK_SIZE: u32 = 512;

/// Synthesizes a `FileAttr` structure for a given inode and size.
///
/// This function constructs a complete FUSE file attribute structure from
/// minimal input parameters. All other fields (timestamps, permissions, type)
/// are derived from the inode number using fixed rules.
///
/// # Attribute Derivation Rules
///
/// - **File type**: Inode 1 -> Directory, others -> RegularFile
/// - **Permissions**: Inode 1 -> 0755, others -> 0644
/// - **Link count**: Inode 1 -> 2 (for `.` and parent), others -> 1
/// - **Timestamps**: All set to Unix epoch (1970-01-01 00:00:00 UTC)
/// - **UID/GID**: Passed as parameters, typically set at mount time
/// - **Block count**: `size.div_ceil(FUSE_BLOCK_SIZE)`
/// - **Block size**: `FUSE_BLOCK_SIZE` (512 bytes)
/// - **Device ID**: 0 (not a device file)
/// - **Flags**: 0 (no extended attributes)
///
/// # Parameters
///
/// - `ino`: Inode number (1=root, 2=disk, 3=memory)
/// - `size`: Logical size in bytes (0 for directories, snapshot/overlay size for files)
/// - `uid`: Owner user ID (typically the mount user's UID)
/// - `gid`: Owner group ID (typically the mount user's GID)
///
/// # Returns
///
/// A fully populated `FileAttr` structure ready to be returned via FUSE
/// reply callbacks (`ReplyEntry`, `ReplyAttr`).
///
/// # Examples
///
/// ```
/// use strata_fuse::vfs::attr::make_attr;
///
/// // Root directory
/// let root = make_attr(1, 0, 1000, 1000);
/// assert_eq!(root.kind, fuser::FileType::Directory);
/// assert_eq!(root.perm, 0o755);
/// assert_eq!(root.nlink, 2);
///
/// // Disk file (10 GiB)
/// let disk = make_attr(2, 10 * 1024 * 1024 * 1024, 1000, 1000);
/// assert_eq!(disk.kind, fuser::FileType::RegularFile);
/// assert_eq!(disk.perm, 0o644);
/// assert_eq!(disk.size, 10 * 1024 * 1024 * 1024);
/// assert_eq!(disk.blocks, (10 * 1024 * 1024 * 1024) / 512);
/// ```
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
