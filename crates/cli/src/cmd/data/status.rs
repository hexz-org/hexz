//! Show status of local changes in a workspace.

use anyhow::Result;
use std::path::PathBuf;
use super::workspace::Workspace;
use walkdir::WalkDir;

/// Lists files that have been modified in the workspace overlay.
pub fn run(path: Option<PathBuf>) -> Result<()> {
    let start_path = path.unwrap_or_else(|| std::env::current_dir().unwrap());
    
    let ws = Workspace::find(&start_path)?
        .ok_or_else(|| anyhow::anyhow!("Not in a hexz workspace (no .hexz found)"))?;

    let overlay = ws.overlay_path();
    
    println!("Workspace: {:?}", ws.root);
    if let Some(b) = ws.config.base_archive {
        println!("Base:      {:?}", b);
    } else {
        println!("Base:      (none)");
    }
    println!("
Changes:");

    let mut found_any = false;
    for entry in WalkDir::new(&overlay).into_iter().filter_map(|e| e.ok()) {
        if entry.path() == overlay { continue; }
        
        let rel = entry.path().strip_prefix(&overlay)?;
        let prefix = if entry.file_type().is_dir() { "  dir:  " } else { "  mod:  " };
        println!("{}{}", prefix, rel.display());
        found_any = true;
    }

    if !found_any {
        println!("  (no local changes)");
    }

    Ok(())
}
