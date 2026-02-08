//! FUSE adapter for mounting Strata snapshots.
//!
//! Exposes a minimal filesystem (root directory plus `disk` and optional
//! `memory` files) over FUSE, with optional overlay support for writable
//! state.

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
