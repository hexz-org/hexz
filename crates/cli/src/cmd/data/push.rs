//! Push archives to a remote endpoint.

use anyhow::{Context, Result};
use colored::Colorize;
use hexz_core::format::header::Header;
use hexz_core::format::magic::HEADER_SIZE;
use hexz_store::StorageBackend;
use hexz_store::local::MmapBackend;
use hexz_store::remote::{self, RemoteTransport};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::workspace::Workspace;

/// Execute the `hexz push` command to upload archives to a remote.
pub fn run(remote_name: &str, archive: Option<PathBuf>) -> Result<()> {
    let ws = Workspace::find(&std::env::current_dir()?)?
        .context("Not in a hexz workspace (no .hexz found)")?;

    let url = ws.config.remotes.get(remote_name).with_context(|| {
        format!(
            "Remote '{remote_name}' not found. Add it with `hexz remote add {remote_name} <url>`"
        )
    })?;

    let target = if let Some(a) = archive {
        a
    } else if let Some(b) = &ws.config.base_archive {
        b.clone()
    } else {
        anyhow::bail!("No archive specified and workspace has no base archive to push.");
    };

    if !target.exists() {
        anyhow::bail!("Archive not found: {}", target.display());
    }

    println!(
        "{} Pushing to    {} {}",
        "╭".dimmed(),
        remote_name.magenta(),
        url.bright_black()
    );

    let transport =
        remote::connect(url).map_err(|e| anyhow::anyhow!("Failed to connect to remote: {e}"))?;

    let mut pushed = HashSet::new();
    push_archive(&target, transport.as_ref(), &mut pushed)?;

    println!("\n  {} Push complete.", "✓".green());
    Ok(())
}

/// Recursively push an archive and any parents missing from the remote.
fn push_archive(
    path: &Path,
    transport: &dyn RemoteTransport,
    pushed: &mut HashSet<PathBuf>,
) -> Result<()> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("Cannot resolve archive path: {}", path.display()))?;

    if !pushed.insert(canonical.clone()) {
        return Ok(());
    }

    let name = canonical
        .file_name()
        .context("Archive path has no file name")?
        .to_string_lossy()
        .to_string();

    // Check if already on remote
    let already_exists = transport
        .exists(&name)
        .map_err(|e| anyhow::anyhow!("Failed to check remote: {e}"))?;

    if already_exists {
        println!("  {} {} (already on remote)", "·".dimmed(), name.dimmed());
    } else {
        // Read header to find parents before uploading
        let header = read_header(&canonical)?;

        // Push parents first (so remote always has a complete chain)
        let archive_dir = canonical.parent().unwrap_or_else(|| Path::new("."));
        for parent_path in &header.parent_paths {
            let parent = resolve_parent(archive_dir, parent_path);
            if parent.exists() {
                push_archive(&parent, transport, pushed)?;
            } else {
                eprintln!(
                    "  {} Parent not found locally: {} (skipping)",
                    "!".yellow(),
                    parent_path
                );
            }
        }

        println!("  {} Uploading {}...", "→".yellow(), name.cyan());
        transport
            .upload(&canonical, &name)
            .map_err(|e| anyhow::anyhow!("Upload failed: {e}"))?;
        println!("  {} {}", "✓".green(), name.green());
    }

    Ok(())
}

/// Read just the header from a local archive file.
fn read_header(path: &Path) -> Result<Header> {
    let backend = MmapBackend::new(path)
        .map_err(|e| anyhow::anyhow!("Cannot open archive {}: {e}", path.display()))?;
    let header_bytes = backend
        .read_exact(0, HEADER_SIZE)
        .map_err(|e| anyhow::anyhow!("Cannot read header: {e}"))?;
    let header: Header = bincode::deserialize(&header_bytes).context("Invalid archive header")?;
    Ok(header)
}

/// Resolve a parent path relative to the archive's directory.
fn resolve_parent(archive_dir: &Path, parent_path: &str) -> PathBuf {
    let p = Path::new(parent_path);
    if p.is_absolute() && p.exists() {
        return p.to_path_buf();
    }
    let rel = archive_dir.join(parent_path);
    if rel.exists() {
        return rel;
    }
    p.to_path_buf()
}
