//! Pull archives from a remote endpoint.

use anyhow::{Context, Result};
use colored::Colorize;
use hexz_core::format::header::Header;
use hexz_core::format::magic::HEADER_SIZE;
use hexz_store::StorageBackend;
use hexz_store::local::MmapBackend;
use hexz_store::remote::{self, RemoteTransport};
use std::collections::HashSet;
use std::path::Path;

use super::workspace::Workspace;

/// Execute the `hexz pull` command to fetch archives from a remote.
pub fn run(remote_name: &str, archive: Option<&str>) -> Result<()> {
    let ws = Workspace::find(&std::env::current_dir()?)?
        .context("Not in a hexz workspace (no .hexz found)")?;

    let url = ws.config.remotes.get(remote_name).with_context(|| {
        format!(
            "Remote '{remote_name}' not found. Add it with `hexz remote add {remote_name} <url>`"
        )
    })?;

    // Determine the local directory where archives are stored
    let local_dir = ws
        .config
        .host_cwd
        .clone()
        .unwrap_or_else(|| ws.root.clone());

    println!(
        "{} Pulling from  {} {}",
        "╭".dimmed(),
        remote_name.magenta(),
        url.bright_black()
    );

    let transport =
        remote::connect(url).map_err(|e| anyhow::anyhow!("Failed to connect to remote: {e}"))?;

    let mut pulled = HashSet::new();

    if let Some(name) = archive {
        // Pull a specific archive
        pull_archive(name, &local_dir, transport.as_ref(), &mut pulled)?;
    } else {
        // Pull all archives not present locally
        println!("  {} Listing remote archives...", "→".yellow());
        let remote_archives = transport
            .list_archives()
            .map_err(|e| anyhow::anyhow!("Failed to list remote archives: {e}"))?;

        if remote_archives.is_empty() {
            println!("  {} No archives on remote.", "·".dimmed());
        } else {
            let mut new_count = 0u32;
            for info in &remote_archives {
                let local_path = local_dir.join(&info.name);
                if local_path.exists() {
                    continue;
                }
                pull_archive(&info.name, &local_dir, transport.as_ref(), &mut pulled)?;
                new_count += 1;
            }
            if new_count == 0 {
                println!("  {} Already up to date.", "·".dimmed());
            }
        }
    }

    println!("\n  {} Pull complete.", "✓".green());
    Ok(())
}

/// Download an archive and recursively pull any missing parents.
fn pull_archive(
    name: &str,
    local_dir: &Path,
    transport: &dyn RemoteTransport,
    pulled: &mut HashSet<String>,
) -> Result<()> {
    if !pulled.insert(name.to_string()) {
        return Ok(());
    }

    let local_path = local_dir.join(name);

    if local_path.exists() {
        println!("  {} {} (already local)", "·".dimmed(), name.dimmed());
    } else {
        println!("  {} Downloading {}...", "→".yellow(), name.cyan());
        transport
            .download(name, &local_path)
            .map_err(|e| anyhow::anyhow!("Download failed: {e}"))?;
        println!("  {} {}", "✓".green(), name.green());
    }

    // Read header to discover parents
    if local_path.exists() {
        if let Ok(header) = read_header(&local_path) {
            for parent_path in &header.parent_paths {
                let parent_name = Path::new(parent_path)
                    .file_name()
                    .map_or_else(|| parent_path.clone(), |f| f.to_string_lossy().to_string());

                let parent_local = local_dir.join(&parent_name);
                if !parent_local.exists() {
                    // Check if parent is on remote before trying to pull
                    let on_remote = transport
                        .exists(&parent_name)
                        .map_err(|e| anyhow::anyhow!("Failed to check remote: {e}"))?;
                    if on_remote {
                        pull_archive(&parent_name, local_dir, transport, pulled)?;
                    } else {
                        eprintln!(
                            "  {} Parent not found on remote: {} (skipping)",
                            "!".yellow(),
                            parent_name
                        );
                    }
                }
            }
        }
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
