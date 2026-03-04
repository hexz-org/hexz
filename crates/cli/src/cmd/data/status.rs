//! Show status of local changes in a workspace.

use anyhow::Result;
use colored::*;
use std::path::PathBuf;
use walkdir::WalkDir;

use super::workspace::Workspace;

pub fn run(path: Option<PathBuf>) -> Result<()> {
    let start_path = path.unwrap_or_else(|| std::env::current_dir().unwrap());

    let ws = Workspace::find(&start_path)?
        .ok_or_else(|| anyhow::anyhow!("Not in a hexz workspace (no .hexz found)"))?;

    let overlay = ws.overlay_path();

    println!("{} Workspace   {}", "╭".dimmed(), ws.root.display().to_string().cyan());
    if let Some(b) = ws.config.base_archive {
        println!("{} Base        {}", "╰".dimmed(), b.display().to_string().bright_black());
    } else {
        println!("{} Base        {}", "╰".dimmed(), "(none)".bright_black());
    }

    let mut changes = Vec::new();
    for entry in WalkDir::new(&overlay).into_iter().filter_map(|e| e.ok()) {
        if entry.path() == overlay { continue; }

        let rel = entry.path().strip_prefix(&overlay)?;
        if entry.file_type().is_dir() {
            changes.push(format!("    {} {} {}", "→".dimmed(), rel.display(), "(dir)".bright_black()));
        } else {
            changes.push(format!("    {} {} {}", "→".dimmed(), rel.display().to_string().green(), "(mod)".bright_black()));
        }
    }

    if changes.is_empty() {
        println!("  {} No changes detected.", "→".green());
    } else {
        println!("  {} Changes detected:", "→".yellow());
        for change in changes {
            println!("{}", change);
        }
    }

    Ok(())
}
