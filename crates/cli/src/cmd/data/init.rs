use anyhow::Result;
use std::path::PathBuf;
use super::workspace::Workspace;

/// Initializes a new empty workspace.
pub fn run(path: Option<PathBuf>) -> Result<()> {
    let target_path = path.unwrap_or_else(|| std::env::current_dir().unwrap());
    
    if target_path.exists() && std::fs::read_dir(&target_path)?.next().is_some() {
        let ws_config = target_path.join(".hexz");
        if !ws_config.exists() {
            println!("Initializing workspace in existing directory...");
        } else {
            anyhow::bail!("Directory already contains a workspace.");
        }
    } else {
        std::fs::create_dir_all(&target_path)?;
        println!("Initializing new workspace at {:?}...", target_path);
    }

    Workspace::init(&target_path, None)?;
    println!("Workspace initialized.");
    Ok(())
}
