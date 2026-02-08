//! Subcommand implementations for the `strata` CLI.
//!
//! Each module corresponds to a top-level subcommand (analyze, boot, commit,
//! create, inspect, install, mount, serve, snap, unmount). The `run` function
//! in each module is invoked by `main` after argument parsing.

/// Offline analysis tools for sizing and deduplication guidance.
#[cfg(feature = "diagnostics")]
pub mod analyze;

/// Storage benchmarks.
#[cfg(feature = "diagnostics")]
pub mod bench;

/// System health checks.
#[cfg(feature = "diagnostics")]
pub mod doctor;

/// High-level snapshot build command with profiles.
pub mod build;

/// Overlay analysis and diffing tool.
#[cfg(feature = "diagnostics")]
pub mod diff;

#[cfg(feature = "signing")]
pub mod keygen;
#[cfg(feature = "signing")]
pub mod sign;
#[cfg(feature = "signing")]
pub mod verify;

/// Boot orchestration for launching VMs from snapshots.
#[cfg(feature = "fuse")]
pub mod boot;

/// Snapshot commit logic for consolidating overlays and memory images.
pub mod commit;

/// Snapshot creation routines for building new `.st` images.
pub mod create;

/// Introspection utilities for existing snapshots.
pub mod inspect;

/// Assisted installation path from ISO to snapshot.
#[cfg(feature = "fuse")]
pub mod install;

/// FUSE-based mounting of snapshots as local filesystems.
#[cfg(feature = "fuse")]
pub mod mount;

/// HTTP server entry points for remote snapshot access.
#[cfg(feature = "server")]
pub mod serve;

/// Live snapshotting of running VMs using QMP.
pub mod snap;

/// Command for unmounting previously mounted Strata instances.
#[cfg(feature = "fuse")]
pub mod unmount;
