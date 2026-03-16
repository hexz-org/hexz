//! Checkout an archive into a writable workspace.

use super::mount;
use super::workspace::Workspace;
use anyhow::Result;
use colored::Colorize;
use std::path::Path;

/// Initializes a workspace and mounts the base archive.
#[allow(unsafe_code)]
pub fn run(archive: &Path, path: &Path) -> Result<()> {
    if path.exists() && std::fs::read_dir(path)?.next().is_some() {
        anyhow::bail!("Directory {} is not empty.", path.display());
    }

    std::fs::create_dir_all(path)?;

    println!(
        "{} Initializing workspace at {}",
        "╭".dimmed(),
        path.display().to_string().cyan()
    );
    let ws = Workspace::init(path, Some(archive.to_path_buf()))?;
    let overlay = ws.overlay_path();

    println!(
        "{} Mounting base archive {}",
        "╰".dimmed(),
        archive.display().to_string().bright_black()
    );
    mount::run(
        &archive.to_string_lossy(),
        path,
        true, // daemon
        None, // cache_size
        // SAFETY: getuid() is always safe to call
        unsafe { libc::getuid() },
        // SAFETY: getgid() is always safe to call
        unsafe { libc::getgid() },
        Some(overlay),
        false, // editable (we already have an overlay)
        Some(&ws.metadata_dir()),
    )?;

    println!("\n  {} Workspace ready.", "✓".green());
    Ok(())
}
