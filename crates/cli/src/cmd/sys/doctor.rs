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

use crate::ui::color::{Palette, palette};

pub fn run() -> Result<()> {
    let p = palette();
    println!(
        "\n  {}Hexz Doctor{} — System Health Check\n",
        p.bold, p.reset
    );

    check_binary("fusermount", &["--version"], p);
    check_binary("qemu-system-x86_64", &["--version"], p);
    check_fuse_support(p);
    check_network(p);

    println!("\n  Diagnosis complete.");
    Ok(())
}

fn check_binary(name: &str, args: &[&str], p: &'static Palette) {
    print!("  Checking {}{}{}... ", p.cyan, name, p.reset);
    match Command::new(name).args(args).output() {
        Ok(output) => {
            if output.status.success() {
                println!("{}OK{}", p.green, p.reset);
            } else {
                println!("{}FAIL{} (exit code {})", p.red, p.reset, output.status);
            }
        }
        Err(_) => println!("{}NOT FOUND{} (please install {})", p.yellow, p.reset, name),
    }
}

fn check_fuse_support(p: &'static Palette) {
    print!(
        "  Checking {}FUSE kernel support{} (/dev/fuse)... ",
        p.cyan, p.reset
    );
    if std::path::Path::new("/dev/fuse").exists() {
        println!("{}OK{}", p.green, p.reset);
    } else {
        println!(
            "{}FAIL{} (device node not found — run {}sudo modprobe fuse{})",
            p.red, p.reset, p.dim, p.reset,
        );
    }
}

fn check_network(p: &'static Palette) {
    print!(
        "  Checking {}network connectivity{} (DNS)... ",
        p.cyan, p.reset
    );
    match std::net::ToSocketAddrs::to_socket_addrs("google.com:80") {
        Ok(_) => println!("{}OK{}", p.green, p.reset),
        Err(e) => println!("{}FAIL{} ({})", p.red, p.reset, e),
    }
}
