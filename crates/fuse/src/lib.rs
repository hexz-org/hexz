//! FUSE adapter for mounting Hexz archives as filesystems.
//!
//! This crate provides a FUSE (Filesystem in Userspace) implementation that
//! mounts Hexz archives as directory trees, enabling standard tools to
//! interact with compressed archives as if they were regular files.

/// Virtual filesystem abstractions (inodes, attributes).
pub mod vfs;

/// FUSE filesystem implementation.
pub mod fuse;

use fuser::MountOption;
use hexz_core::Archive;
use std::sync::Arc;

/// Mounts a Hexz archive at a given path using the `fuser` library.
///
/// **Architectural intent:** Creates a read-only filesystem view over an
/// archive so tools can interact with it via standard POSIX operations.
pub fn mount_fs(
    snap: Arc<Archive>,
    mountpoint: &std::path::Path,
    uid: u32,
    gid: u32,
    write_layer: Option<std::path::PathBuf>,
    metadata_dir: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let mut options = vec![
        MountOption::FSName("hexz".to_string()),
        MountOption::AutoUnmount,
    ];

    if write_layer.is_none() {
        options.push(MountOption::RO);
        options.push(MountOption::DefaultPermissions);
    }

    let fs = fuse::Hexz::new(snap, uid, gid, write_layer, metadata_dir)?;
    fuser::mount2(fs, mountpoint, &options)?;
    Ok(())
}
