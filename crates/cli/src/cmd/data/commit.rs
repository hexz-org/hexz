//! Commit changes from a writable mount to a new thin archive.

use anyhow::{Context, Result};
use hexz_ops::pack::{PackConfig, pack_archive};
use std::path::PathBuf;
use std::process::Command;

use super::workspace::Workspace;

/// Commits changes from a writable mount to a new thin archive.
pub fn run(
    output: PathBuf,
    mountpoint: Option<PathBuf>,
    base: Option<PathBuf>,
) -> Result<()> {
    let mountpoint = if let Some(m) = mountpoint {
        std::fs::canonicalize(m)?
    } else {
        // Try to find workspace in CWD
        if let Some(ws) = Workspace::find(&std::env::current_dir()?)? {
            ws.root
        } else {
            anyhow::bail!("No mountpoint provided and no .hexz workspace found.");
        }
    };

    // Try to infer base archive if not provided
    let base = if let Some(b) = base {
        Some(b)
    } else {
        // Check workspace first
        if let Some(ws) = Workspace::find(&mountpoint)? {
            Some(ws.config.base_archive)
        } else {
            infer_base_archive(&mountpoint)
        }
    };

    println!("Committing changes from {:?} to {:?}...", mountpoint, output);
    if let Some(ref b) = base {
        println!("Using base archive: {:?}", b);
    }

    let config = PackConfig {
        input: mountpoint,
        base,
        output,
        compression: "zstd".to_string(), // Default to zstd for commits
        use_dcam: true,
        show_progress: true,
        ..Default::default()
    };

    pack_archive(config, None::<fn(u64, u64)>).context("Commit failed during packing")?;

    println!("Commit complete.");
    Ok(())
}

fn infer_base_archive(mountpoint: &std::path::Path) -> Option<PathBuf> {
    // On Linux, we can try to find the archive path from the mount options
    if cfg!(target_os = "linux") {
        let output = Command::new("findmnt")
            .arg("-n")
            .arg("-o")
            .arg("SOURCE")
            .arg(mountpoint)
            .output()
            .ok()?;

        if output.status.success() {
            let source = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let p = PathBuf::from(source);
            if p.exists() && p.extension().is_some_and(|ext| ext == "hxz") {
                return Some(p);
            }
        }
    }
    None
}
