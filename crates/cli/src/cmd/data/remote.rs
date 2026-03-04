use anyhow::{Context, Result};
use std::path::PathBuf;
use super::workspace::Workspace;
use crate::args::RemoteCommand;

pub fn run(action: RemoteCommand) -> Result<()> {
    let mut ws = Workspace::find(&std::env::current_dir()?)?
        .context("Not in a hexz workspace (no .hexz found)")?;

    match action {
        RemoteCommand::Add { name, url } => {
            ws.config.remotes.insert(name.clone(), url.clone());
            ws.save()?;
            println!("Added remote '{}' -> '{}'", name, url);
        }
        RemoteCommand::Remove { name } => {
            if ws.config.remotes.remove(&name).is_some() {
                ws.save()?;
                println!("Removed remote '{}'", name);
            } else {
                anyhow::bail!("Remote '{}' not found", name);
            }
        }
        RemoteCommand::List => {
            if ws.config.remotes.is_empty() {
                println!("No remotes configured.");
            } else {
                for (name, url) in &ws.config.remotes {
                    println!("{}	{}", name, url);
                }
            }
        }
    }

    Ok(())
}
