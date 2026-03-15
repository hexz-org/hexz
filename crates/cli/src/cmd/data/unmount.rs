//! Unmounting of FUSE-mounted Hexz filesystems.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use colored::Colorize;

/// Unmounts a previously mounted Hexz filesystem.
pub fn run(mountpoint: &Path) -> Result<()> {
    let path_str = mountpoint.to_string_lossy();

    if cfg!(target_os = "linux") {
        let output = Command::new("fusermount")
            .arg("-u")
            .arg(mountpoint)
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                println!("  {} Successfully unmounted {}", "✓".green(), path_str.cyan());
                return Ok(());
            }

            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not found") {
                return Ok(());
            }
        }
    }

    let output = Command::new("umount")
        .arg(mountpoint)
        .output()
        .context("Failed to execute unmount command")?;

    if output.status.success() {
        println!("  {} Successfully unmounted {}", "✓".green(), path_str.cyan());
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not mounted") {
            return Ok(());
        }

        eprint!("{stderr}");
        anyhow::bail!("Failed to unmount {path_str}.");
    }
}
