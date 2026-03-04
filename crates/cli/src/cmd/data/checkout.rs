//! Checkout an archive into a writable workspace.

use anyhow::Result;
use std::path::PathBuf;
use super::workspace::Workspace;
use super::mount;

/// Initializes a workspace and mounts the base archive.
pub fn run(archive: PathBuf, path: PathBuf) -> Result<()> {
    if path.exists() && std::fs::read_dir(&path)?.next().is_some() {
        anyhow::bail!("Directory {:?} is not empty.", path);
    }

    std::fs::create_dir_all(&path)?;

    println!("Initializing workspace at {:?}...", path);
    println!("(Workspace version: 0.7.0-refactor-perms)");
    let ws = Workspace::init(&path, Some(archive.clone()))?;
    let overlay = ws.overlay_path();

    println!("Mounting base archive...");
    mount::run(
        archive.to_string_lossy().to_string(),
        path,
        true, // daemon
        None, // cache_size
        unsafe { libc::getuid() }, // uid
        unsafe { libc::getgid() }, // gid
        Some(overlay),
        false, // editable (we already have an overlay)
        Some(ws.metadata_dir()),
    )?;

    println!("Workspace ready.");
    Ok(())
}
