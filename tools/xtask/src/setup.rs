use crate::common::*;
use anyhow::{Result, bail};

#[derive(clap::Subcommand)]
pub enum SetupCmd {
    /// Check that required system dependencies are present
    Check,
    /// Install development tools (Rust components, cargo tools, Python venv)
    Install,
    /// Add cross-compilation targets and print linker install instructions
    Cross,
}

pub fn run(cmd: SetupCmd) -> Result<()> {
    match cmd {
        SetupCmd::Check => check(),
        SetupCmd::Install => install(),
        SetupCmd::Cross => cross(),
    }
}

fn check() -> Result<()> {
    let mut missing = Vec::new();

    print!("  rustup        ");
    if which::which("rustup").is_ok() {
        println!("{}", check_mark(true));
    } else {
        println!("{} {RED}not found{RESET}", check_mark(false));
        missing.push("rustup (Rust toolchain)");
    }

    print!("  cargo         ");
    if which::which("cargo").is_ok() {
        println!("{}", check_mark(true));
    } else {
        println!("{} {RED}not found{RESET}", check_mark(false));
        missing.push("cargo");
    }

    print!("  pkg-config    ");
    if which::which("pkg-config").is_ok() {
        println!("{}", check_mark(true));
    } else {
        println!("{} {RED}not found{RESET}", check_mark(false));
        missing.push("pkg-config");
    }

    print!("  python3       ");
    if which::which("python3").is_ok() || which::which("python").is_ok() {
        println!("{}", check_mark(true));
    } else {
        println!("{} {RED}not found{RESET}", check_mark(false));
        missing.push("python3");
    }

    print!("  FUSE headers  ");
    let fuse_ok = check_fuse();
    if fuse_ok {
        println!("{}", check_mark(true));
    } else {
        println!("{} {RED}not found{RESET}", check_mark(false));
        missing.push("libfuse (FUSE dev headers)");
    }

    println!();

    if !missing.is_empty() {
        println!("{BOLD}Missing required packages:{RESET}");
        for m in &missing {
            println!("  {m}");
        }
        println!();
        println!("Install them first, then run {BOLD}cargo xtask setup install{RESET} again.\n");
        println!("{CYAN}Examples:{RESET}");
        println!("  Rust:           https://rustup.rs  ->  curl -sSf https://sh.rustup.rs | sh");
        println!(
            "  Ubuntu/Debian:  sudo apt-get update && sudo apt-get install -y pkg-config libfuse-dev python3 python3-venv"
        );
        println!("  Arch:           sudo pacman -S --needed base-devel fuse3 pkg-config python");
        println!("  Fedora:         sudo dnf install pkg-config fuse-devel python3");
        println!("  macOS:          brew install pkg-config macfuse; Rust from https://rustup.rs");
        println!("\nOn Windows use WSL and follow the Ubuntu/Debian line.");
        bail!("missing required system packages");
    }

    println!("{GREEN}All required system packages found.{RESET}");
    Ok(())
}

fn check_fuse() -> bool {
    // Try pkg-config first
    if cmd("pkg-config")
        .arg("--exists")
        .arg("fuse")
        .run_with_status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return true;
    }

    let os = std::env::consts::OS;
    match os {
        "linux" => {
            // Debian/Ubuntu
            if which::which("dpkg").is_ok()
                && cmd("dpkg")
                    .arg("-s")
                    .arg("libfuse-dev")
                    .run_with_status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            {
                return true;
            }
            // Arch
            if which::which("pacman").is_ok() {
                if cmd("pacman")
                    .arg("-Q")
                    .arg("fuse3")
                    .run_with_status()
                    .map(|s| s.success())
                    .unwrap_or(false)
                {
                    return true;
                }
                if cmd("pacman")
                    .arg("-Q")
                    .arg("fuse2")
                    .run_with_status()
                    .map(|s| s.success())
                    .unwrap_or(false)
                {
                    return true;
                }
            }
            false
        }
        "macos" => {
            which::which("brew").is_ok()
                && cmd("brew")
                    .arg("list")
                    .arg("macfuse")
                    .run_with_status()
                    .map(|s| s.success())
                    .unwrap_or(false)
        }
        _ => false,
    }
}

fn install() -> Result<()> {
    check()?;

    println!("{GREEN}Installing development tools\u{2026}{RESET}");

    cmd("rustup")
        .args(["component", "add", "rustfmt", "clippy"])
        .run()?;

    cmd(cargo())
        .args(["install", "cargo-deny", "cargo-fuzz", "maturin", "critcmp"])
        .run()?;

    let root = find_workspace_root()?;
    let venv = root.join(".venv");
    if !venv.exists() {
        println!("{GREEN}Creating Python venv\u{2026}{RESET}");
        let status = cmd("python3")
            .args(["-m", "venv"])
            .arg(&venv)
            .run_with_status();
        if status.map(|s| s.success()).unwrap_or(false) {
            // ok
        } else {
            cmd("python").args(["-m", "venv"]).arg(&venv).run()?;
        }
    }

    let req_file = root.join("docs/requirements.txt");
    if req_file.exists() {
        let pip = venv.join("bin/pip");
        if pip.exists() {
            let _ = cmd(pip.to_str().unwrap())
                .args(["install", "-q", "-r"])
                .arg(req_file.to_str().unwrap())
                .run_with_status();
        }
    }

    println!("{GREEN}Done. Run 'make check' to verify.{RESET}");
    Ok(())
}

fn cross() -> Result<()> {
    println!("{GREEN}Adding Rust cross-compilation targets\u{2026}{RESET}");
    cmd("rustup")
        .args([
            "target",
            "add",
            "aarch64-unknown-linux-gnu",
            "x86_64-pc-windows-gnu",
        ])
        .run()?;

    println!("\n{BOLD}System cross-compilers also required:{RESET}");
    println!("  {CYAN}Arch:{RESET}     sudo pacman -S aarch64-linux-gnu-gcc mingw-w64-gcc");
    println!(
        "  {CYAN}Ubuntu:{RESET}   sudo apt install gcc-aarch64-linux-gnu gcc-mingw-w64-x86-64"
    );
    println!("  {CYAN}Fedora:{RESET}   sudo dnf install gcc-aarch64-linux-gnu-gcc mingw64-gcc");
    println!("\nThen run {BOLD}make pre-release{RESET} to validate.");
    Ok(())
}
