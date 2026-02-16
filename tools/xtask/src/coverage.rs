use crate::common::*;
use anyhow::Result;

const COV_IGNORE_REGEX: &str = "(tests/|benches/|py_interface/|cmd/vm/boot\\.rs|cmd/vm/install\\.rs|cmd/vm/unmount\\.rs|cmd/sys/serve\\.rs|loader/src/lib\\.rs|tensor/numpy\\.rs)";

#[derive(clap::Subcommand)]
pub enum CoverageCmd {
    /// Run both Rust and Python coverage
    All,
    /// Rust coverage via cargo-llvm-cov
    Rust,
    /// Python coverage via pytest-cov
    Python,
}

pub fn run(cmd: CoverageCmd) -> Result<()> {
    match cmd {
        CoverageCmd::All => {
            rust_cov()?;
            python_cov()
        }
        CoverageCmd::Rust => rust_cov(),
        CoverageCmd::Python => python_cov(),
    }
}

fn ensure_llvm_cov() -> Result<()> {
    if which::which("cargo-llvm-cov").is_err() {
        println!("{CYAN}Installing cargo-llvm-cov\u{2026}{RESET}");
        cmd(cargo()).args(["install", "cargo-llvm-cov"]).run()?;
    }
    Ok(())
}

fn rust_cov() -> Result<()> {
    let root = find_workspace_root()?;
    ensure_llvm_cov()?;

    println!("{GREEN}Generating Rust coverage report\u{2026}{RESET}");
    println!("{CYAN}Running Rust tests (may take a moment)\u{2026}{RESET}");

    let output = cmd(cargo())
        .args([
            "llvm-cov",
            "--workspace",
            "--ignore-filename-regex",
            COV_IGNORE_REGEX,
            "--color=always",
        ])
        .current_dir(&root)
        .capture_all()?;

    // Filter output: keep from "^Filename" onward, drop info/Compiling/Finished/Running lines
    let mut past_header = false;
    for line in output.lines() {
        if line.starts_with("Filename") {
            past_header = true;
        }
        if past_header {
            if line.starts_with("info:")
                || line.starts_with("  -->")
                || line.starts_with("   Compiling")
                || line.starts_with("    Finished")
                || line.starts_with("     Running")
            {
                continue;
            }
            println!("{line}");
        }
    }

    Ok(())
}

fn python_cov() -> Result<()> {
    let root = find_workspace_root()?;
    let loader_crate = root.join("crates/loader");
    let python_bin = python(&root);

    println!("{GREEN}Generating Python coverage report\u{2026}{RESET}");
    println!("{CYAN}Building Python extension (may take a moment)\u{2026}{RESET}");

    cmd(maturin())
        .args(["develop", "-q", "-E", "test,numpy"])
        .current_dir(&loader_crate)
        .run()?;

    println!("{CYAN}Running Python tests (may take a moment)\u{2026}{RESET}");

    let output = cmd(python_bin.as_str())
        .args([
            "-m",
            "pytest",
            "tests/",
            "--cov=python/hexz",
            "--cov-report=term-missing",
            "--tb=no",
            "--color=yes",
            "-q",
            "-p",
            "no:warnings",
        ])
        .current_dir(&loader_crate)
        .capture_all()?;

    // Filter output: drop pip progress, extract coverage table and test summary
    let mut in_cov = false;
    for line in output.lines() {
        // Skip pip/install noise
        if line.starts_with("Requirement already satisfied")
            || line.starts_with("Collecting")
            || line.starts_with("Downloading")
            || line.starts_with("Installing")
            || line.starts_with("Successfully installed")
            || line.contains('\u{270F}')
            || line.starts_with("Ignoring")
        {
            continue;
        }

        if line.starts_with("Name") {
            in_cov = true;
        }
        if in_cov {
            println!("{line}");
            if line.starts_with("TOTAL") {
                in_cov = false;
                println!();
            }
        } else if line.contains("passed")
            || line.contains("failed")
            || line.contains("error")
            || line.contains("skipped")
        {
            println!("{line}");
        }
    }

    Ok(())
}
