//! Initialize a new Hexz workspace.

use anyhow::{Context, Result};
use std::path::PathBuf;
use colored::Colorize;
use super::workspace::Workspace;

/// Initializes a new empty workspace.
pub fn run(path: Option<PathBuf>) -> Result<()> {
    let target_path = path.unwrap_or(std::env::current_dir().context("Failed to get current directory")?);
    
    if target_path.exists() && std::fs::read_dir(&target_path)?.next().is_some() {
        let ws_config = target_path.join(".hexz");
        if ws_config.exists() {
            anyhow::bail!("Directory already contains a workspace.");
        }
        println!("{} Initializing workspace in existing directory {}", "╭".dimmed(), target_path.display().to_string().cyan());
    } else {
        std::fs::create_dir_all(&target_path)?;
        println!("{} Initializing new workspace at {}", "╭".dimmed(), target_path.display().to_string().cyan());
    }

    let _ = Workspace::init(&target_path, None)?;
    println!("{} Workspace initialized.", "╰".dimmed());
    Ok(())
}
