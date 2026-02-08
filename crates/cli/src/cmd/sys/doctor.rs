//! Implementation of the `strata doctor` command.
//!
//! Checks system health, dependencies, and configuration for troubleshooting.

use anyhow::Result;
use std::process::Command;

pub fn run() -> Result<()> {
    println!("Strata Doctor - System Health Check\n");

    check_binary("fusermount", &["--version"]);
    check_binary("qemu-system-x86_64", &["--version"]);
    check_fuse_support();
    check_network();

    println!("\nDiagnosis complete.");
    Ok(())
}

fn check_binary(name: &str, args: &[&str]) {
    print!("Checking {}... ", name);
    match Command::new(name).args(args).output() {
        Ok(output) => {
            if output.status.success() {
                println!("OK");
                // Optionally print version
                // let s = String::from_utf8_lossy(&output.stdout);
                // println!("  Version: {}", s.lines().next().unwrap_or("unknown").trim());
            } else {
                println!("FAIL (Exit code {})", output.status);
            }
        }
        Err(_) => println!("NOT FOUND (Please install {})", name),
    }
}

fn check_fuse_support() {
    print!("Checking FUSE kernel support (/dev/fuse)... ");
    if std::path::Path::new("/dev/fuse").exists() {
        println!("OK");
    } else {
        println!("FAIL (Device node not found. Is fuse module loaded?)");
    }
}

fn check_network() {
    print!("Checking Network Connectivity (DNS)... ");
    // Try to resolve google.com using std lib
    match std::net::ToSocketAddrs::to_socket_addrs("google.com:80") {
        Ok(_) => println!("OK"),
        Err(e) => println!("FAIL ({})", e),
    }
}
