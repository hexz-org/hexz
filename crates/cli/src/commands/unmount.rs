//! Unmounting of FUSE-mounted Strata filesystems.
//!
//! Invokes the platform unmount tool (`fusermount -u` on Linux, `umount`
//! elsewhere) to detach a mount point previously created by the mount
//! subcommand. Does not modify the snapshot or overlay files.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

/// Unmounts a previously mounted Strata filesystem.
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
