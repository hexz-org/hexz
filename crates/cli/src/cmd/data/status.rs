//! Show status of local changes in a workspace.

use anyhow::{Context, Result};
use colored::Colorize;
use std::path::PathBuf;
use walkdir::WalkDir;

use super::workspace::Workspace;

/// Execute the `hexz status` command to show workspace changes.
pub fn run(path: Option<PathBuf>) -> Result<()> {
    let start_path =
        path.unwrap_or(std::env::current_dir().context("Failed to get current directory")?);

    let ws = Workspace::find(&start_path)?
        .ok_or_else(|| anyhow::anyhow!("Not in a hexz workspace (no .hexz found)"))?;

    let overlay = ws.overlay_path();

    println!(
        "{} Workspace   {}",
        "╭".dimmed(),
        ws.root.display().to_string().cyan()
    );
    if let Some(b) = ws.config.base_archive {
        println!(
            "{} Base        {}",
            "╰".dimmed(),
            b.display().to_string().bright_black()
        );
    } else {
        println!("{} Base        {}", "╰".dimmed(), "(none)".bright_black());
    }

    let mut changes = Vec::new();
    for entry in WalkDir::new(&overlay)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        if entry.path() == overlay {
            continue;
        }

        let rel = entry.path().strip_prefix(&overlay)?;
        if entry.file_type().is_dir() {
            changes.push(format!(
                "    {} {} {}",
                "→".dimmed(),
                rel.display(),
                "(dir)".bright_black()
            ));
        } else {
            changes.push(format!(
                "    {} {} {}",
                "→".dimmed(),
                rel.display().to_string().green(),
                "(mod)".bright_black()
            ));
        }
    }

    if changes.is_empty() {
        println!("  {} No changes detected.", "→".green());
    } else {
        println!("  {} Changes detected:", "→".yellow());
        for change in changes {
            println!("{change}");
        }
    }

    Ok(())
}
