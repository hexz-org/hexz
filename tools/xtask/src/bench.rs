use crate::common::*;
use anyhow::{Result, bail};
use walkdir::WalkDir;

#[derive(clap::Subcommand)]
pub enum BenchCmd {
    /// List available benchmark categories
    List,
}

pub fn run(cmd: BenchCmd) -> Result<()> {
    match cmd {
        BenchCmd::List => list(),
    }
}

fn list() -> Result<()> {
    let root = find_workspace_root()?;
    let criterion_dir = root.join("target/criterion");
    let bench_store = root.join(".criterion");

    // Find a directory with benchmark data
    let bench_dir = if criterion_dir.is_dir() {
        criterion_dir
    } else if bench_store.is_dir() {
        // Use first subdir of .criterion/
        let mut found = None;
        if let Ok(entries) = std::fs::read_dir(&bench_store) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    found = Some(entry.path());
                    break;
                }
            }
        }
        match found {
            Some(d) => d,
            None => bail!(
                "{BOLD}No criterion data. Run 'make bench' or have a baseline in .criterion/{RESET}"
            ),
        }
    } else {
        bail!("{BOLD}No criterion data. Run 'make bench' or have a baseline in .criterion/{RESET}");
    };

    println!(
        "{GREEN}Benchmark categories (use with make bench <category> or bench-compare <baseline> <category>)\u{2026}{RESET}\n"
    );

    let mut categories = std::collections::BTreeSet::new();
    let prefix = bench_dir.to_string_lossy().to_string();

    for entry in WalkDir::new(&bench_dir).into_iter().flatten() {
        if entry.file_name() == "benchmark.json" {
            let path = entry.path().to_string_lossy();
            if let Some(relative) = path.strip_prefix(&prefix) {
                let relative = relative.trim_start_matches('/');
                if let Some(category) = relative.split('/').next() {
                    categories.insert(category.to_string());
                }
            }
        }
    }

    for cat in &categories {
        println!("{cat}");
    }

    Ok(())
}
