//! FUSE filesystem implementation for Strata snapshots.
//!
//! This module implements the [`fuser::Filesystem`] trait, translating POSIX
//! filesystem operations into reads/writes on Strata snapshots and overlays.
//!
//! # Architecture
//!
//! The FUSE layer consists of three handler modules:
//!
//! - [`lookup`]: Inode lookup, directory listing, attribute queries
//! - [`read`]: Read operations on disk/memory files
//! - [`write`]: Write operations (redirected to overlay)
//!
//! # Inode Structure
//!
//! The filesystem uses a fixed inode layout:
//!
//! | Inode | Type      | Name     | Description                |
//! |-------|-----------|----------|----------------------------|
//! | 1     | Directory | `.`      | Root directory             |
//! | 2     | File      | `disk`   | Disk stream (main data)    |
//! | 3     | File      | `memory` | Memory stream (if present) |
//!
//! # Operation Routing
//!
//! ```text
//! User Operation (read /mnt/snapshot/disk)
//!         │
//!         ↓
//! Kernel FUSE Driver
//!         │
//!         ↓
//! fuser::Filesystem::read()
//!         │
//!         ↓
//! read::handle_read()
//!         │
//!    ┌────┴────┐
//!    ↓         ↓
//! Overlay  StrataFile
//! (if set) (base snapshot)
//! ```
//!
//! # Thread Safety
//!
//! The [`Strata`] filesystem struct is `!Send` due to FUSE constraints but
//! uses `Arc<StrataFile>` internally, allowing the snapshot to be shared
//! across threads outside the FUSE context.

mod lookup;
mod read;
mod write;

use crate::vfs::{InodeMap, InodeType, Overlay};
use fuser::{FileAttr, Filesystem};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use strata_core::StrataFile;

/// Attribute and entry cache TTL reported to the FUSE kernel module (1 second).
///
/// **Architectural intent:** Short TTL allows the kernel to revalidate
/// attributes and directory entries after overlay or snapshot changes without
/// requiring manual cache invalidation.
const TTL: Duration = Duration::from_secs(1);

/// Block size in bytes used for overlay-backed size and block count (512).
///
/// **Architectural intent:** Aligns with the FUSE `blksize` and `st_blocks`
/// semantics so that the exported disk file reports consistent block counts
/// when overlay length exceeds the base snapshot.
const FUSE_BLOCK_SIZE: u64 = 512;

/// FUSE filesystem adapter for Strata snapshots.
///
/// **Architectural intent:** Combines a `StrataFile`, inode layout, and overlay
/// state into a single object that satisfies the `Filesystem` trait.
///
/// **Constraints:** The overlay path, if present, is stored for the lifetime
/// of the mount to allow metadata persistence on drop.
pub struct Strata {
    pub(crate) snap: Arc<StrataFile>,
    pub(crate) inodes: InodeMap,
    pub(crate) overlay: Option<Overlay>,
    pub(crate) overlay_path: Option<std::path::PathBuf>,
}

impl Strata {
    /// Constructs a FUSE filesystem from a `StrataFile` and optional overlay.
    ///
    /// **Architectural intent:** Encapsulates the logic for opening the
    /// overlay file and building the inode map so that `mount_fs` remains a
    /// thin wrapper.
    ///
    /// **Constraints:** When an `overlay_path` is supplied it must be
    /// creatable and writable by the process; failures propagate as
    /// `anyhow::Error`.
    ///
    /// **Side effects:** Opens the overlay file on disk (if configured) and
    /// clones the snapshot handle, incrementing its reference count.
    pub fn new(
        snap: Arc<StrataFile>,
        overlay_path: Option<&Path>,
        uid: u32,
        gid: u32,
    ) -> anyhow::Result<Self> {
        let overlay = if let Some(p) = overlay_path {
            Some(Overlay::new(p)?)
        } else {
            None
        };

        Ok(Self {
            snap: snap.clone(),
            inodes: InodeMap::new(&snap, uid, gid),
            overlay,
            overlay_path: overlay_path.map(|p| p.to_path_buf()),
        })
    }

    /// Returns inode attributes after reconciling base snapshot and overlay.
    ///
    /// **Architectural intent:** Ensures that the exported disk file reports a
    /// size and block count that reflect any appended data stored in the
    /// overlay rather than the immutable base snapshot.
    ///
    /// **Constraints:** Only inode `2` is merged with overlay information; all
    /// other inodes use attributes derived solely from the snapshot.
    ///
    /// **Side effects:** May query overlay file metadata, which incurs a
    /// filesystem stat operation per call.
    pub(crate) fn get_merged_attr(&self, ino: u64) -> FileAttr {
        let mut attr = self.inodes.getattr(ino);
        if ino == InodeType::Disk as u64 {
            if let Some(ov) = &self.overlay {
                let ov_len = ov.len();
                if ov_len > attr.size {
                    attr.size = ov_len;
                    attr.blocks = attr.size.div_ceil(FUSE_BLOCK_SIZE);
                }
            }
        }
        attr
    }
}

impl Drop for Strata {
    /// Persists overlay metadata when the filesystem is dropped.
    ///
    /// **Architectural intent:** Ensures that the set of modified blocks is
    /// flushed to disk so that subsequent mounts can reconstruct the same
    /// overlay view.
    ///
    /// **Constraints:** Best-effort only; failures are ignored and reported
    /// via logs at most, since drop cannot reliably propagate errors.
    ///
    /// **Side effects:** May perform synchronous writes to the overlay metadata
    /// file during drop.
    fn drop(&mut self) {
        if let (Some(ov), Some(path)) = (&self.overlay, &self.overlay_path) {
            let _ = ov.save_metadata(path);
        }
    }
}

impl Filesystem for Strata {
    fn lookup(
        &mut self,
        req: &fuser::Request,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEntry,
    ) {
        lookup::handle_lookup(self, req, parent, name, reply);
    }

    fn getattr(&mut self, req: &fuser::Request, ino: u64, reply: fuser::ReplyAttr) {
        lookup::handle_getattr(self, req, ino, reply);
    }

    fn setattr(
        &mut self,
        req: &fuser::Request,
        ino: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<fuser::TimeOrNow>,
        mtime: Option<fuser::TimeOrNow>,
        ctime: Option<std::time::SystemTime>,
        fh: Option<u64>,
        crtime: Option<std::time::SystemTime>,
        chgtime: Option<std::time::SystemTime>,
        bkuptime: Option<std::time::SystemTime>,
        flags: Option<u32>,
        reply: fuser::ReplyAttr,
    ) {
        lookup::handle_setattr(
            self, req, ino, mode, uid, gid, size, atime, mtime, ctime, fh, crtime, chgtime,
            bkuptime, flags, reply,
        );
    }

    fn readdir(
        &mut self,
        req: &fuser::Request,
        ino: u64,
        fh: u64,
        offset: i64,
        reply: fuser::ReplyDirectory,
    ) {
        lookup::handle_readdir(self, req, ino, fh, offset, reply);
    }

    fn read(
        &mut self,
        req: &fuser::Request,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        flags: i32,
        lock: Option<u64>,
        reply: fuser::ReplyData,
    ) {
        read::handle_read(self, req, ino, fh, offset, size, flags, lock, reply);
    }

    fn write(
        &mut self,
        req: &fuser::Request,
        ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        write_flags: u32,
        flags: i32,
        lock: Option<u64>,
        reply: fuser::ReplyWrite,
    ) {
        write::handle_write(
            self,
            req,
            ino,
            fh,
            offset,
            data,
            write_flags,
            flags,
            lock,
            reply,
        );
    }
}
