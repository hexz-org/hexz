use crate::common::*;
use anyhow::Result;
use std::collections::BTreeSet;

#[derive(clap::Subcommand)]
pub enum TestCmd {
    /// List Rust test categories for filtering
    List,
    /// Exercise every CLI command and flag combination
    Commands,
}

pub fn run(cmd: TestCmd) -> Result<()> {
    match cmd {
        TestCmd::List => list(),
        TestCmd::Commands => commands(),
    }
}

fn list() -> Result<()> {
    let root = find_workspace_root()?;

    let output = cmd(cargo())
        .args(["test", "--workspace", "--", "--list"])
        .current_dir(&root)
        .capture_all()?;

    println!("{GREEN}Rust test categories (use with make test <category>)\u{2026}{RESET}\n");

    let mut categories = BTreeSet::new();
    for line in output.lines() {
        if let Some(rest) = line.strip_suffix(": test") {
            if let Some(category) = rest.split("::").next() {
                categories.insert(category.to_string());
            }
        }
    }

    for cat in &categories {
        println!("{cat}");
    }

    Ok(())
}

fn commands() -> Result<()> {
    let root = find_workspace_root()?;
    let bin = root.join("target/release/hexz");

    if !bin.exists() {
        println!("{CYAN}Building hexz (release)\u{2026}{RESET}");
        cmd(cargo())
            .args(["build", "--release", "--workspace"])
            .current_dir(&root)
            .run()?;
    }

    let tmp = tempfile::tempdir()?;
    let tmp_path = tmp.path();
    let bin_str = bin.to_str().unwrap();

    // 1. Create test file and directory
    let test_file = tmp_path.join("test.bin");
    std::fs::write(&test_file, "Hello Hexz File")?;

    let test_dir = tmp_path.join("test_dir");
    std::fs::create_dir_all(&test_dir)?;
    std::fs::write(test_dir.join("a.txt"), "File A")?;
    std::fs::write(test_dir.join("b.txt"), "File B")?;

    println!("{CYAN}=== Hexz Archive Operations Test ==={RESET}");

    // [1] Pack file
    println!("{CYAN}[1] Pack file{RESET}");
    let file_hxz = tmp_path.join("file.hxz");
    cmd(bin_str)
        .args([
            "pack",
            file_hxz.to_str().unwrap(),
            "--input",
            test_file.to_str().unwrap(),
        ])
        .run()?;

    // [2] Pack directory
    println!("{CYAN}[2] Pack directory{RESET}");
    let dir_hxz = tmp_path.join("dir.hxz");
    cmd(bin_str)
        .args([
            "pack",
            dir_hxz.to_str().unwrap(),
            "--input",
            test_dir.to_str().unwrap(),
        ])
        .run()?;

    // [3] Show/Inspect
    println!("{CYAN}[3] Show archive{RESET}");
    cmd(bin_str)
        .args(["show", dir_hxz.to_str().unwrap()])
        .run()?;

    // [4] Extract
    println!("{CYAN}[4] Extract archive{RESET}");
    let extracted_dir = tmp_path.join("extracted");
    cmd(bin_str)
        .args([
            "extract",
            dir_hxz.to_str().unwrap(),
            extracted_dir.to_str().unwrap(),
        ])
        .run()?;
    
    assert!(extracted_dir.join("a.txt").exists());
    assert_eq!(std::fs::read_to_string(extracted_dir.join("a.txt"))?, "File A");

    // [5] Thin delta
    println!("{CYAN}[5] Create thin delta{RESET}");
    std::fs::write(test_dir.join("c.txt"), "File C")?;
    let delta_hxz = tmp_path.join("delta.hxz");
    cmd(bin_str)
        .args([
            "pack",
            delta_hxz.to_str().unwrap(),
            "--input",
            test_dir.to_str().unwrap(),
            "--base",
            dir_hxz.to_str().unwrap(),
        ])
        .run()?;

    // [6] Log
    println!("{CYAN}[6] Log lineage{RESET}");
    cmd(bin_str).args(["log", tmp_path.to_str().unwrap()]).run()?;

    // [7] Diff
    println!("{CYAN}[7] Diff archives{RESET}");
    cmd(bin_str)
        .args([
            "diff",
            dir_hxz.to_str().unwrap(),
            delta_hxz.to_str().unwrap(),
        ])
        .run()?;

    println!("{GREEN}All command tests passed!{RESET}");
    Ok(())
}
