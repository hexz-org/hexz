//! Virtual machine operation commands for snapshot-based VMs.
//!
//! This module provides commands for managing virtual machines backed by Hexz
//! snapshots, enabling instant boot, live snapshotting, and layered storage.
//!
//! # Available Commands
//!
//! ## VM Lifecycle (feature = "fuse")
//!
//! - [`boot`]: Boot a VM from a snapshot
//!   - Mounts snapshot via FUSE
//!   - Launches QEMU/Firecracker hypervisor
//!   - Supports persistent overlays for writes
//!
//! - [`install`]: Install OS from ISO to create a new snapshot
//!   - Creates virtual disk
//!   - Runs installer in VM
//!   - Packs result into Hexz archive
//!
//! ## Snapshot Management
//!
//! - [`snap`]: Create live snapshot via QMP (QEMU Machine Protocol)
//!   - Captures running VM state without downtime
//!   - Uses dirty block tracking for efficiency
//!
//! - [`commit`]: Commit overlay changes to new snapshot
//!   - Merges overlay deltas with base snapshot
//!   - Optionally flattens entire chain
//!   - Supports thin snapshots (reference base)
//!
//! ## Filesystem Operations (feature = "fuse")
//!
//! - [`mount`]: Mount snapshot as read-only or read-write filesystem
//!   - FUSE-based transparent decompression
//!   - Copy-on-write overlay for writes
//!   - NBD export support
//!
//! - [`unmount`]: Unmount FUSE filesystem
//!
//! # Workflow Example
//!
//! ```bash
//! # 1. Install OS from ISO
//! hexz vm install --iso ubuntu.iso --disk-size 20G --output ubuntu.st
//!
//! # 2. Boot with persistent overlay
//! hexz vm boot ubuntu.st --persist overlay.bin
//!
//! # (Make changes in VM, then shut down)
//!
//! # 3. Commit changes to new snapshot
//! hexz vm commit ubuntu.st overlay.bin ubuntu-v2.st --message "Installed updates"
//!
//! # 4. Boot new snapshot
//! hexz vm boot ubuntu-v2.st
//! ```
//!
//! # Live Snapshot with QMP
//!
//! ```bash
//! # Boot with QMP socket
//! hexz vm boot ubuntu.st --qmp-socket /tmp/qmp.sock --persist overlay.bin
//!
//! # Create live snapshot (VM keeps running)
//! hexz vm snap --socket /tmp/qmp.sock --base ubuntu.st \
//!   --overlay overlay.bin --output checkpoint.st
//! ```
//!
//! # Thin Snapshots
//!
//! Thin snapshots only store deltas and reference the base:
//!
//! ```bash
//! hexz vm commit base.st overlay.bin delta.st --thin
//! # delta.st is small, but requires base.st to boot
//! ```
//!
//! # Feature Requirements
//!
//! - `fuse`: boot, install, mount, unmount
//! - Core commands (snap, commit) always available

#[cfg(feature = "fuse")]
pub mod boot;

#[cfg(feature = "fuse")]
pub mod install;

pub mod commit;
pub mod snap;

#[cfg(feature = "fuse")]
pub mod mount;

#[cfg(feature = "fuse")]
pub mod unmount;
