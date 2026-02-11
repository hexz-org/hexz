//! Command handlers for the Strata CLI.
//!
//! This module organizes all CLI subcommands into logical groups following a
//! **Noun-Verb hierarchy** (e.g., `strata data pack`, `strata vm boot`). Each
//! submodule contains the implementation logic for its respective commands.
//!
//! # Architecture
//!
//! ```text
//! strata <CATEGORY> <ACTION> [OPTIONS]
//!        ^^^^^^^^^^  ^^^^^^
//!        module      function
//! ```
//!
//! # Command Categories
//!
//! - [`data`]: Data operations for archives
//!   - `pack`: Create archives from disk images
//!   - `build`: Build archives from directories
//!   - `info`: Inspect archive metadata
//!   - `diff`: Show overlay differences (diagnostics feature)
//!   - `analyze`: Optimize CDC parameters with DCAM (diagnostics feature)
//!
//! - [`vm`]: Virtual machine operations
//!   - `boot`: Launch VMs from snapshots (FUSE feature)
//!   - `install`: Install OS from ISO (FUSE feature)
//!   - `snap`: Create live snapshots via QMP
//!   - `commit`: Commit overlay changes
//!   - `mount`: Mount snapshots as filesystems (FUSE feature)
//!   - `unmount`: Unmount filesystems (FUSE feature)
//!
//! - [`sys`]: System utilities
//!   - `doctor`: Run diagnostics (diagnostics feature)
//!   - `bench`: Benchmark performance (diagnostics feature)
//!   - `serve`: Serve archives over network (server feature)
//!   - `keygen`: Generate signing keys (signing feature)
//!   - `sign`: Sign archives (signing feature)
//!   - `verify`: Verify signatures (signing feature)
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
