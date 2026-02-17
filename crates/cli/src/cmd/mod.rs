//! Command handlers for the Hexz CLI.
//!
//! This module organizes all CLI subcommands into logical groups. While the CLI
//! itself uses a **flat command structure** (e.g., `hexz pack`, `hexz boot`),
//! the implementation is internally namespaced to keep the code organized.
//!
//! # Architecture
//!
//! ```text
//! hexz <COMMAND> [OPTIONS]
//!      ^^^^^^^^^
//!      flat command (grouped by category in help)
//! ```
//!
//! # Command Categories
//!
//! - **Archive Operations** (`cmd::data`)
//!   - `pack`: Create archives from disk images
//!   - `build`: Build archives from directories
//!   - `info`: Inspect archive metadata
//!   - `diff`: Show overlay differences
//!   - `analyze`: Optimize CDC parameters
//!
//! - **Virtual Machine Operations** (`cmd::vm`)
//!   - `boot`: Launch VMs from snapshots
//!   - `install`: Install OS from ISO
//!   - `snap`: Create live snapshots via QMP
//!   - `commit`: Commit overlay changes
//!   - `mount`: Mount snapshots as filesystems
//!   - `unmount`: Unmount filesystems
//!
//! - **System & Diagnostics** (`cmd::sys`)
//!   - `doctor`: Run diagnostics
//!   - `bench`: Benchmark performance
//!   - `serve`: Serve archives over network
//!   - `keygen`: Generate signing keys
//!   - `sign`: Sign archives
//!   - `verify`: Verify signatures
//!
//! # Error Handling
//!
//! All command handlers return `anyhow::Result<()>`, with errors automatically
//! propagated to the main entry point for user-friendly display.
//!
//! # Feature Flags
//!
//! Some commands are gated behind feature flags:
//! - `diagnostics`: Advanced debugging and analysis tools
//! - `fuse`: Filesystem mounting capabilities
//! - `server`: Network serving protocols
//! - `signing`: Cryptographic signing and verification

pub mod data;
pub mod sys;
pub mod vm;
