//! Manage remote endpoints for push/pull operations.

use super::workspace::Workspace;
use crate::args::RemoteCommand;
use anyhow::{Context, Result};
use colored::Colorize;

/// Execute the `hexz remote` command to manage remote endpoints.
pub fn run(action: RemoteCommand) -> Result<()> {
    let mut ws = Workspace::find(&std::env::current_dir()?)?
        .context("Not in a hexz workspace (no .hexz found)")?;

    match action {
        RemoteCommand::Add { name, url } => {
            let _ = ws.config.remotes.insert(name.clone(), url.clone());
            ws.save()?;
            println!(
                "  {} Added remote {} {}",
                "✓".green(),
                name.magenta(),
                format!("({url})").bright_black()
            );
        }
        RemoteCommand::Remove { name } => {
            if ws.config.remotes.remove(&name).is_some() {
                ws.save()?;
                println!("  {} Removed remote {}", "✓".green(), name.magenta());
            } else {
                anyhow::bail!("Remote '{name}' not found");
            }
        }
        RemoteCommand::List => {
            if ws.config.remotes.is_empty() {
                println!("  {} No remotes configured.", "→".yellow());
            } else {
                println!("{} Remotes", "╭".dimmed());
                let count = ws.config.remotes.len();
                for (i, (name, url)) in ws.config.remotes.iter().enumerate() {
                    let prefix = if i == count - 1 {
                        "╰".dimmed()
                    } else {
                        "│".dimmed()
                    };
                    println!("{} {} {}", prefix, name.magenta(), url.bright_black());
                }
            }
        }
    }

    Ok(())
}
