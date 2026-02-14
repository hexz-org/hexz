//! Unmounting of FUSE-mounted Hexz filesystems.
//!
//! This command safely detaches Hexz snapshots that were mounted as filesystems
//! using the `mount` command. It uses platform-specific unmount tools and handles
//! error cases gracefully.
//!
//! # Unmount Strategy
//!
//! The command tries unmount methods in order:
//!
//! 1. **Linux**: `fusermount -u` (preferred for FUSE mounts)
//! 2. **Fallback**: `umount` (generic unmount tool)
//!
//! # Usage
//!
//! ```bash
//! # Unmount a previously mounted snapshot
//! hexz vm unmount /mnt/snapshot
//! ```
//!
//! # Error Handling
//!
//! The command handles several cases gracefully:
//! - **Not mounted**: Returns success (already unmounted)
//! - **Busy**: Reports error if mountpoint is in use
//! - **Permission denied**: Reports error if insufficient privileges
//!
//! # Safety
//!
//! - Does not modify the snapshot or overlay files
//! - Safe to run even if already unmounted
//! - Does not affect other mounts
//!
//! # Overlay Persistence
//!
//! If the mount was created with an overlay (`--overlay`), the overlay file
//! remains intact after unmounting and can be used for future mounts or commits.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

/// Unmounts a previously mounted Hexz filesystem.
///
/// **Architectural intent:** Attempts to detach the FUSE mount using the
/// platform-preferred tool (`fusermount -u` on Linux), falling back to
/// `umount` when necessary.
///
/// **Constraints:** The `mountpoint` must refer to a valid mount; both
/// commands rely on external system utilities and their presence on `$PATH`.
///
/// **Side effects:** Spawns subprocesses to invoke unmount operations and
/// prints status messages to stdout; on failure, returns a descriptive error.
pub fn run(mountpoint: PathBuf) -> Result<()> {
    let path_str = mountpoint.to_string_lossy();

    if cfg!(target_os = "linux")
        && let Ok(output) = Command::new("fusermount")
            .arg("-u")
            .arg(&mountpoint)
            .output()
    {
        if output.status.success() {
            println!("Successfully unmounted {}", path_str);
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not found") {
            return Ok(());
        }
    }

    let output = Command::new("umount")
        .arg(&mountpoint)
        .output()
        .context("Failed to execute unmount command")?;

    if output.status.success() {
        println!("Successfully unmounted {}", path_str);
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not mounted") {
            return Ok(());
        }

        eprint!("{}", stderr);
        anyhow::bail!("Failed to unmount {}.", path_str);
    }
}
