//! FUSE adapter for mounting Strata snapshots as filesystems.
//!
//! This crate provides a FUSE (Filesystem in Userspace) implementation that
//! mounts Strata snapshots as block device files, enabling standard tools to
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
//! use strata_fuse::mount_fs;
//! use strata_core::StrataFile;
//! use strata_core::store::local::FileBackend;
//! use strata_core::algo::compression::lz4::Lz4Compressor;
//! use std::sync::Arc;
//! use std::path::Path;
//!
//! # fn main() -> anyhow::Result<()> {
//! // Open snapshot
//! let backend = Arc::new(FileBackend::new("snapshot.st".as_ref())?);
//! let compressor = Box::new(Lz4Compressor::new());
//! let snap = StrataFile::new(backend, compressor, None)?;
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
//! - Commit: Use `strata vm commit` to merge overlay into new snapshot
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
pub mod vfs;

/// FUSE filesystem implementation.
pub mod fuse;

use fuser::MountOption;
use std::path::Path;
use std::sync::Arc;
use strata_core::StrataFile;

/// Mounts a Strata snapshot at a given path using the `fuser` library.
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
    snap: Arc<StrataFile>,
    mountpoint: &Path,
    overlay_path: Option<&Path>,
    uid: u32,
    gid: u32,
) -> anyhow::Result<()> {
    let options = vec![
        MountOption::RW,
        MountOption::FSName("strata".to_string()),
        MountOption::DefaultPermissions,
    ];

    let fs = fuse::Strata::new(snap, overlay_path, uid, gid)?;
    fuser::mount2(fs, mountpoint, &options)?;
    Ok(())
}
