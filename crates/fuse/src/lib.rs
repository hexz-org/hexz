//! FUSE adapter for mounting Hexz snapshots as filesystems.
//!
//! This crate provides a FUSE (Filesystem in Userspace) implementation that
//! mounts Hexz snapshots as block device files, enabling standard tools to
//! interact with compressed archives as if they were regular files.
//!
//! # Overview
//!
//! The FUSE adapter exposes a minimal filesystem structure:
//!
//! ```text
//! /mountpoint/
//! ├── disk       (block device file, size = snapshot disk size)
//! └── memory     (optional, if memory stream present)
//! ```
//!
//! # Features
//!
//! - **Transparent Decompression**: Reads decompress blocks on-the-fly
//! - **Overlay Support**: Writes go to separate overlay file (copy-on-write)
//! - **Standard Tools**: Works with dd, qemu, mount, parted, etc.
//! - **Random Access**: Efficient seeking without full decompression
//!
//! # Usage
//!
//! ```no_run
//! use hexz_fuse::mount_fs;
//! use hexz_core::File;
//! use hexz_core::store::local::FileBackend;
//! use hexz_core::algo::compression::lz4::Lz4Compressor;
//! use std::sync::Arc;
//! use std::path::Path;
//!
//! # fn main() -> anyhow::Result<()> {
//! // Open snapshot
//! let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
//! let compressor = Box::new(Lz4Compressor::new());
//! let snap = File::new(backend, compressor, None)?;
//!
//! // Mount with overlay
//! mount_fs(snap, Path::new("/mnt/snapshot"), Some(Path::new("overlay.bin")), 1000, 1000)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Overlay Copy-on-Write
//!
//! When mounted with an overlay:
//! - Reads: Overlay deltas override base snapshot
//! - Writes: Stored in overlay file, base remains immutable
//! - Commit: Use `hexz vm commit` to merge overlay into new snapshot
//!
//! # Performance
//!
//! - **Read latency**: ~80μs (cached), ~1ms (uncached)
//! - **Write latency**: ~50μs (overlay write, no compression)
//! - **Sequential throughput**: ~2-3 GB/s with LZ4
//!
//! # Requirements
//!
//! - Linux with FUSE support (`fusermount` or `libfuse`)
//! - User must have permission to mount filesystems

/// Virtual filesystem abstractions (inodes, attributes, overlay).
///
/// - [`vfs::inode`]: Inode numbering, directory entries, inode metadata
/// - [`vfs::attr`]: File attribute construction (size, mode, timestamps)
/// - [`vfs::overlay`]: Copy-on-write overlay for writable mounts
///
/// Format details: See [`vfs::overlay::Overlay`]
pub mod vfs;

/// FUSE filesystem implementation.
///
/// - [`fuse::lookup`]: Inode lookup, directory listing, attribute queries
/// - [`fuse::read`]: Read operations on disk/memory files
///
/// The [`fuse::Hexz`] filesystem struct is `!Send` due to FUSE constraints but
pub mod fuse;

use fuser::MountOption;
use hexz_core::File;
use std::path::Path;
use std::sync::Arc;

/// Mounts a Hexz snapshot at a given path using the `fuser` library.
///
/// **Architectural intent:** Creates a read-mostly filesystem view over a
/// snapshot and optional overlay so tools can interact with it via standard
/// POSIX operations.
///
/// **Constraints:** The target `mountpoint` must exist and be accessible to
/// the caller. Options are fixed to read-write with default permission
/// handling; additional mount flags are not currently surfaced.
///
/// **Side effects:** Spawns a FUSE background thread inside `fuser::mount2`
/// and holds open file descriptors for the snapshot and overlay for the
/// lifetime of the mount.
pub fn mount_fs(
    snap: Arc<File>,
    mountpoint: &Path,
    overlay_path: Option<&Path>,
    uid: u32,
    gid: u32,
) -> anyhow::Result<()> {
    let options = vec![
        MountOption::RW,
        MountOption::FSName("hexz".to_string()),
        MountOption::DefaultPermissions,
    ];

    let fs = fuse::Hexz::new(snap, overlay_path, uid, gid)?;
    fuser::mount2(fs, mountpoint, &options)?;
    Ok(())
}
