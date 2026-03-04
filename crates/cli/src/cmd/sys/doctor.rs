//! Implementation of the `hexz doctor` command.
//!
//! Performs comprehensive system health checks to verify that all required
//! dependencies, kernel modules, and system resources are available for Hexz
//! operations. This command helps troubleshoot installation issues and validates
//! the environment before running VMs or mounting archives.
//!
//! # Common Usage Patterns
//!
//! ```bash
//! hexz doctor
//! hexz doctor > hexz-diagnostics.txt   # plain text for bug reports
//! ```

use anyhow::Result;
use std::process::Command;
use colored::*;

use crate::ui::color::{Palette, palette};

pub fn run() -> Result<()> {
    let p = palette();
    println!("{} Hexz Doctor", "╭".dimmed());
    println!("{} System Health Check", "╰".dimmed());
    println!();

    check_binary("fusermount", &["--version"], p);
    check_binary("qemu-system-x86_64", &["--version"], p);
    check_fuse_support(p);
    check_network(p);

    println!("\n  {} Diagnosis complete.", "✓".green());
    Ok(())
}

fn check_binary(name: &str, args: &[&str], _p: &'static Palette) {
    print!("  {} Checking {}... ", "→".yellow(), name.cyan());
    match Command::new(name).args(args).output() {
        Ok(output) => {
            if output.status.success() {
                println!("{}", "OK".green());
            } else {
                println!("{} (exit code {})", "FAIL".red(), output.status);
            }
        }
        Err(_) => println!("{} (please install {})", "NOT FOUND".yellow(), name),
    }
}

fn check_fuse_support(_p: &'static Palette) {
    print!(
        "  {} Checking {} (/dev/fuse)... ",
        "→".yellow(),
        "FUSE kernel support".cyan()
    );
    if std::path::Path::new("/dev/fuse").exists() {
        println!("{}", "OK".green());
    } else {
        println!(
            "{} (device node not found — run sudo modprobe fuse)",
            "FAIL".red()
        );
    }
}

fn check_network(_p: &'static Palette) {
    print!(
        "  {} Checking {} (DNS)... ",
        "→".yellow(),
        "network connectivity".cyan()
    );
    match std::net::ToSocketAddrs::to_socket_addrs("google.com:80") {
        Ok(_) => println!("{}", "OK".green()),
        Err(e) => println!("{} ({})", "FAIL".red(), e),
    }
}
