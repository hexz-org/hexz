//! OS installation from ISO and conversion to Strata snapshot.
//!
//! This command automates the process of installing an operating system from
//! an ISO image and converting the result into a Strata snapshot archive.
//!
//! # Installation Workflow
//!
//! 1. **Create Virtual Disk**: Generate temporary raw disk image
//! 2. **Launch Installer**: Boot QEMU with ISO attached
//! 3. **User Interaction**: User completes OS installation
//! 4. **Shutdown VM**: User powers off after installation
//! 5. **Pack Snapshot**: Convert raw disk to compressed `.st` archive
//! 6. **Cleanup**: Remove temporary raw disk
//!
//! # Usage Example
//!
//! ```bash
//! # Install Ubuntu with 20GB disk and 4GB RAM
//! strata vm install --iso ubuntu-24.04.iso \
//!   --disk-size 20G --ram 4G --output ubuntu.st
//!
//! # Install with VNC (for headless servers)
//! strata vm install --iso debian.iso --disk-size 10G \
//!   --ram 2G --output debian.st --vnc
//!
//! # Install with CDC for better deduplication
//! strata vm install --iso alpine.iso --disk-size 5G \
//!   --ram 1G --output alpine.st --cdc
//! ```
//!
//! # Requirements
//!
//! - `qemu-img`: For creating virtual disks
//! - `qemu-system-x86_64`: For running the installer VM
//! - Sufficient disk space for temporary raw image (= `disk_size`)
//!
//! # Performance Notes
//!
//! - Installation speed depends on ISO and hardware
//! - Packing uses LZ4 compression by default (fast)
//! - Add `--cdc` for better deduplication (slower but smaller)

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

use crate::cmd::data::pack;

/// Number of vCPUs passed to QEMU for the installer VM (2).
///
/// **Architectural intent:** Provides enough parallelism for typical
/// installers without over-provisioning; passed as `-smp` to QEMU.
const QEMU_SMP_COUNT: &str = "2";

/// Block size in bytes used when creating the snapshot from the raw disk (64 KiB).
///
/// **Architectural intent:** Matches the default compression block size for
/// the create pipeline; changing it affects output layout and compression
/// granularity.
const DEFAULT_BLOCK_SIZE: u32 = 65536;

/// Installs an operating system from an ISO and converts it into a snapshot.
///
/// **Architectural intent:** Automates the multi-step workflow of creating a
/// raw disk, running a QEMU-based installer, and feeding the resulting disk
/// through the standard snapshot creation pipeline.
///
/// **Constraints:** Requires `qemu-img` and `qemu-system-x86_64` to be
/// installed and in `$PATH`. The `disk_size` and `ram` parameters are passed
/// directly to QEMU tooling and must use size suffixes they understand (for
/// example `10G`, `4G`).
///
/// **Side effects:** Creates and deletes temporary disk images, spawns a full
/// virtual machine for the duration of installation, and writes the final
/// snapshot to `output`.
pub fn run(
    iso: PathBuf,
    disk_size: String,
    ram: String,
    output: PathBuf,
    no_graphics: bool,
    vnc: bool,
    cdc: bool,
) -> Result<()> {
    println!("Creating temporary raw disk ({})...", disk_size);
    let temp_dir = tempfile::tempdir()?;
    let raw_path = temp_dir.path().join("temp_install.raw");

    let status = Command::new("qemu-img")
        .arg("create")
        .arg("-f")
        .arg("raw")
        .arg(&raw_path)
        .arg(&disk_size)
        .status()
        .context("Failed to create raw disk. Is qemu-img installed?")?;

    if !status.success() {
        anyhow::bail!("Failed to create raw disk image");
    }

    println!("Starting Installer. Please install the OS and SHUT DOWN when finished.");
    println!("NOTE: Networking is DISABLED to ensure a clean, isolated snapshot.");

    let mut cmd = Command::new("qemu-system-x86_64");

    cmd.arg("-m")
        .arg(&ram)
        .arg("-enable-kvm")
        .arg("-smp")
        .arg(QEMU_SMP_COUNT);

    if vnc {
        println!("Starting VNC server on display :1 (Port 5901).");
        println!("Connect via SSH tunnel: ssh -L 5901:localhost:5901 <host>");
        cmd.arg("-display").arg("vnc=:1");
    } else if no_graphics {
        println!("(Running in Headless Serial Mode)");
        println!("* To exit QEMU: Press 'Ctrl+a' then 'x'");
        println!(
            "* IMPORTANT: You may need to edit the kernel boot line in GRUB and add 'console=ttyS0'"
        );

        cmd.arg("-nographic");
    }

    let status = cmd
        .arg("-net")
        .arg("none")
        .arg("-cdrom")
        .arg(&iso)
        .arg("-drive")
        .arg(format!("file={},format=raw", raw_path.display()))
        .status()
        .context("Failed to run QEMU installer")?;

    if !status.success() {
        anyhow::bail!("QEMU installer exited with error");
    }

    println!("Installation finished (VM shut down).");
    println!("Converting raw disk to Strata snapshot...");

    pack::run(
        Some(raw_path.clone()),
        None,
        output.clone(),
        "lz4".to_string(),
        false,
        false,
        DEFAULT_BLOCK_SIZE,
        cdc,
        16384,
        65536,
        131072,
        false,
    )?;

    println!("Cleanup complete.");
    println!("Created: {:?}", output);
    println!("You can now boot this with: strata boot {:?}", output);

    Ok(())
}
