use crate::common::*;
use anyhow::Result;
use walkdir::WalkDir;

pub fn run() -> Result<()> {
    let root = find_workspace_root()?;
    println!("{GREEN}Cleaning\u{2026}{RESET}");

    cmd(cargo()).arg("clean").current_dir(&root).run()?;

    // Remove loader build/dist dirs
    let loader_build = root.join("crates/loader/build");
    let loader_dist = root.join("crates/loader/dist");
    remove_if_exists(&loader_build);
    remove_if_exists(&loader_dist);

    // Walk and remove Python artifacts
    let mut dirs_to_remove = Vec::new();
    let mut files_to_remove = Vec::new();

    for entry in WalkDir::new(&root)
        .into_iter()
        .filter_entry(|e| {
            // Skip .git and target dirs for speed
            let name = e.file_name().to_string_lossy();
            name != ".git" && name != "target" && name != ".venv"
        })
        .flatten()
    {
        let name = entry.file_name().to_string_lossy();
        if entry.file_type().is_dir() {
            if name == "__pycache__" || name.ends_with(".egg-info") || name == ".eggs" {
                dirs_to_remove.push(entry.into_path());
            }
        } else if name.ends_with(".pyc") || name.ends_with(".pyo") {
            files_to_remove.push(entry.into_path());
        }
    }

    for f in files_to_remove {
        let _ = std::fs::remove_file(&f);
    }
    for d in dirs_to_remove {
        let _ = std::fs::remove_dir_all(&d);
    }

    // Remove target/wheels
    remove_if_exists(&root.join("target/wheels"));

    println!("{GREEN}Clean complete.{RESET}");
    Ok(())
}

fn remove_if_exists(path: &std::path::Path) {
    if path.exists() {
        let _ = std::fs::remove_dir_all(path);
    }
}
