//! Mount an archive and spawn a subshell.

use anyhow::{Context, Result};
use colored::Colorize;
use hexz_fuse::fuse::Hexz;
use std::path::PathBuf;
use std::process::Command;

use super::mount::open_archive;

/// Execute the `hexz shell` command to mount an archive and spawn a subshell.
#[allow(unsafe_code)]
pub fn run(
    hexz_path: &str,
    overlay: Option<PathBuf>,
    editable: bool,
    cache_size: Option<&str>,
) -> Result<()> {
    // SAFETY: getuid() is always safe to call
    let uid = unsafe { libc::getuid() };
    // SAFETY: getgid() is always safe to call
    let gid = unsafe { libc::getgid() };

    let snap = open_archive(hexz_path, cache_size, None)?;

    // Create temp mountpoint
    let tmp_dir = tempfile::tempdir().context("Failed to create temporary directory")?;
    let mountpoint = tmp_dir.path().to_path_buf();

    // Create temp metadata dir for workspace config
    let tmp_meta = tempfile::tempdir().context("Failed to create temporary metadata directory")?;
    let metadata_dir = tmp_meta.path().to_path_buf();

    // Handle --editable / --overlay
    let (overlay, is_temp_overlay) = if let Some(o) = overlay {
        std::fs::create_dir_all(&o)?;
        (Some(o), false)
    } else if editable {
        let temp_overlay = tempfile::tempdir().context("Failed to create temporary overlay")?;
        let path = temp_overlay.path().to_path_buf();
        let _ = temp_overlay.keep();
        (Some(path), true)
    } else {
        (None, false)
    };

    // Initialize workspace config so `hexz status` works
    {
        let host_cwd = std::env::current_dir().ok();
        let config = crate::cmd::data::workspace::WorkspaceConfig {
            base_archive: Some(std::fs::canonicalize(hexz_path)?),
            overlay_path: overlay.clone(),
            host_cwd,
            remotes: std::collections::HashMap::new(),
        };
        let config_path = metadata_dir.join("config.json");
        let f = std::fs::File::create(config_path)?;
        serde_json::to_writer_pretty(f, &config)?;
    }

    let fs = Hexz::new(snap, uid, gid, overlay.clone(), Some(&metadata_dir))?;

    let mut options = vec![
        fuser::MountOption::FSName("hexz".to_string()),
        fuser::MountOption::DefaultPermissions,
    ];

    if overlay.is_none() {
        options.push(fuser::MountOption::RO);
    }

    println!(
        "  {} Mounting archive at {}",
        "→".yellow(),
        mountpoint.display().to_string().cyan()
    );
    if let Some(ref o) = overlay {
        println!(
            "  {} Using overlay: {}",
            "→".yellow(),
            o.display().to_string().bright_black()
        );
    }

    // Mount in background thread
    let mountpoint_clone = mountpoint.clone();
    let options_clone = options.clone();
    drop(std::thread::spawn(move || {
        let _ = fuser::mount2(fs, mountpoint_clone, &options_clone);
    }));

    // Give FUSE a moment to actually mount
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Spawn shell
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    println!(
        "  {} Dropping into shell: {}",
        "→".yellow(),
        shell.bright_black()
    );
    println!(
        "  {} Type {} to unmount and exit.",
        "→".yellow(),
        "exit".bold()
    );

    let status = Command::new(&shell)
        .current_dir(&mountpoint)
        .status()
        .context("Failed to spawn shell")?;

    if !status.success() {
        eprintln!("  {} Shell exited with status: {}", "✗".red(), status);
    }

    // Cleanup
    println!("  {} Unmounting...", "→".yellow());

    // Attempt to unmount
    #[cfg(target_os = "linux")]
    {
        // Try fusermount3 first, then fusermount
        if Command::new("fusermount3")
            .arg("-u")
            .arg(&mountpoint)
            .status()
            .is_err()
        {
            let _ = Command::new("fusermount")
                .arg("-u")
                .arg(&mountpoint)
                .status();
        }
    }
    #[cfg(target_os = "macos")]
    let _ = Command::new("umount").arg(&mountpoint).status();

    if is_temp_overlay {
        if let Some(o) = overlay {
            let _ = std::fs::remove_dir_all(o);
        }
    }

    Ok(())
}
