//! Implementation of the `hexz doctor` command.
//!
//! Performs comprehensive system health checks to verify that all required
//! dependencies, kernel modules, and system resources are available for Hexz
//! operations. This command helps troubleshoot installation issues and validates
//! the environment before running VMs or mounting snapshots.
//!
//! # Diagnostic Checks Performed
//!
//! ## Binary Dependencies
//!
//! **`fusermount`:**
//! - Required for: FUSE mounting (`hexz mount`)
//! - Checks: Command exists and returns version
//! - Failure mode: Command not found or non-zero exit code
//!
//! **`qemu-system-x86_64`:**
//! - Required for: VM booting (`hexz boot`)
//! - Checks: Command exists and returns version
//! - Failure mode: Command not found or non-zero exit code
//!
//! ## Kernel Support
//!
//! **FUSE Kernel Module (`/dev/fuse`):**
//! - Required for: FUSE mounting
//! - Checks: Device node exists
//! - Failure mode: Device not found (module not loaded)
//! - Fix: `sudo modprobe fuse`
//!
//! ## Network Connectivity
//!
//! **DNS Resolution:**
//! - Required for: Remote backends (HTTP, S3), package updates
//! - Checks: Can resolve `google.com` to socket address
//! - Failure mode: DNS lookup fails (network down, DNS misconfigured)
//!
//! # Interpretation of Results
//!
//! **OK:**
//! - Component is installed and working correctly
//! - No action required
//!
//! **NOT FOUND:**
//! - Binary is not in `$PATH`
//! - Action: Install the required package
//!   - `fusermount`: Install `fuse` or `fuse3` package
//!   - `qemu-system-x86_64`: Install `qemu-system-x86` package
//!
//! **FAIL:**
//! - Binary exists but returned error or unexpected behavior
//! - Action: Check binary is correct version or reinstall
//!
//! **FAIL (Device node not found):**
//! - Kernel module not loaded
//! - Action: Load module with `sudo modprobe fuse`
//!
//! **FAIL (DNS error):**
//! - Network connectivity issue
//! - Action: Check network configuration, DNS settings
//!
//! # Use Cases
//!
//! - **Pre-Installation Verification**: Confirm all dependencies before first use
//! - **Troubleshooting**: Diagnose why commands are failing
//! - **CI/CD Validation**: Verify build environments have required tools
//! - **Bug Reports**: Include `doctor` output in issue reports
//!
//! # Common Usage Patterns
//!
//! ```bash
//! # Run comprehensive health check
//! hexz doctor
//!
//! # Include in bug reports
//! hexz doctor > hexz-diagnostics.txt
//!
//! # Verify installation
//! sudo apt install qemu-system-x86 fuse3
//! hexz doctor  # Should show all OK
//! ```

use anyhow::Result;
use std::process::Command;

/// Executes the doctor command to perform system health checks.
///
/// Runs a series of diagnostic tests to verify that all required dependencies,
/// kernel modules, and system resources are available. Prints results for each
/// check and provides guidance on fixing issues.
///
/// # Output Format
///
/// ```text
/// Hexz Doctor - System Health Check
///
/// Checking fusermount... OK
/// Checking qemu-system-x86_64... OK
/// Checking FUSE kernel support (/dev/fuse)... OK
/// Checking Network Connectivity (DNS)... OK
///
/// Diagnosis complete.
/// ```
///
/// # Errors
///
/// This command does not return errors. All diagnostic failures are reported
/// as "FAIL" or "NOT FOUND" in the output, but the command itself succeeds.
/// This allows the function to complete all checks even if some fail.
///
/// # Examples
///
/// ```no_run
/// use hexz_cli::cmd::sys::doctor;
///
/// // Run system diagnostics
/// doctor::run()?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run() -> Result<()> {
    println!("Hexz Doctor - System Health Check\n");

    check_binary("fusermount", &["--version"]);
    check_binary("qemu-system-x86_64", &["--version"]);
    check_fuse_support();
    check_network();

    println!("\nDiagnosis complete.");
    Ok(())
}

/// Checks if a binary exists and runs successfully.
///
/// Executes the specified command with given arguments and reports whether
/// it exists and returns successfully.
///
/// # Arguments
///
/// * `name` - Name of the binary to check (must be in `$PATH`)
/// * `args` - Arguments to pass (typically `["--version"]`)
///
/// # Output
///
/// - "OK" if command exists and returns zero exit code
/// - "FAIL (Exit code N)" if command exists but fails
/// - "NOT FOUND (Please install ...)" if command not in `$PATH`
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

/// Checks if the FUSE kernel module is loaded.
///
/// Verifies that `/dev/fuse` exists, which indicates the FUSE kernel module
/// is loaded and ready for use.
///
/// # Output
///
/// - "OK" if `/dev/fuse` exists
/// - "FAIL (Device node not found. Is fuse module loaded?)" otherwise
fn check_fuse_support() {
    print!("Checking FUSE kernel support (/dev/fuse)... ");
    if std::path::Path::new("/dev/fuse").exists() {
        println!("OK");
    } else {
        println!("FAIL (Device node not found. Is fuse module loaded?)");
    }
}

/// Checks network connectivity via DNS resolution.
///
/// Attempts to resolve `google.com` to verify DNS and basic network connectivity.
/// This is a simple connectivity test that doesn't require external network access
/// beyond DNS.
///
/// # Output
///
/// - "OK" if DNS resolution succeeds
/// - "FAIL (error message)" if resolution fails
fn check_network() {
    print!("Checking Network Connectivity (DNS)... ");
    // Try to resolve google.com using std lib
    match std::net::ToSocketAddrs::to_socket_addrs("google.com:80") {
        Ok(_) => println!("OK"),
        Err(e) => println!("FAIL ({})", e),
    }
}
