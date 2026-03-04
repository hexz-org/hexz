//! Commit changes from a writable mount to a new thin archive.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;
use colored::*;
use indicatif::HumanBytes;

use hexz_ops::pack::{PackConfig, pack_archive};

use super::workspace::Workspace;

pub fn run(
    mut output: PathBuf,
    mountpoint: Option<PathBuf>,
    base: Option<PathBuf>,
) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let ws = Workspace::find(&current_dir)?;

    let mountpoint = if let Some(m) = mountpoint {
        std::fs::canonicalize(m)?
    } else {
        // Try to find workspace in CWD
        if let Some(ref w) = ws {
            w.root.clone()
        } else {
            anyhow::bail!("No mountpoint provided and no .hexz workspace found.");
        }
    };

    // If output is relative and we have a host_cwd, resolve it
    if output.is_relative() {
        if let Some(ref w) = ws {
            if let Some(ref host_cwd) = w.config.host_cwd {
                output = host_cwd.join(&output);
            }
        }
    }

    // Try to infer base archive if not provided
    let base = if let Some(b) = base {
        Some(b)
    } else {
        // Check workspace first
        if let Some(ref w) = ws {
            if let Some(b) = w.config.base_archive.clone() {
                Some(b)
            } else {
                infer_base_archive(&mountpoint)
            }
        } else {
            infer_base_archive(&mountpoint)
        }
    };

    println!("{} Committing to {}", "╭".dimmed(), output.display().to_string().cyan());
    if let Some(ref b) = base {
        println!("{} Base:         {}", "╰".dimmed(), b.display().to_string().bright_black());
    } else {
        println!("{} Base:         {}", "╰".dimmed(), "(none)".bright_black());
    }

    let config = PackConfig {
        input: mountpoint,
        base,
        output: output.clone(),
        compression: "zstd".to_string(), // Default to zstd for commits
        use_dcam: true,
        show_progress: true,
        ..Default::default()
    };

    pack_archive(config, None::<fn(u64, u64)>).context("Commit failed during packing")?;

    let file_size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    let size_str = HumanBytes(file_size).to_string();

    println!("\n  {} Commit complete {}", "✓".green(), format!("({} delta)", size_str).bright_black());
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
