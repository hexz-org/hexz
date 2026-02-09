//! Command-line argument definitions for the Strata CLI.
//!
//! This module defines all Clap argument structures using a nested "Noun-Verb"
//! command hierarchy (e.g., `strata data pack`, `strata vm boot`).
//!
//! **Design principle:** Arguments are defined separately from handlers to keep
//! CLI structure clear and testable. The actual command implementations live in
//! the `cmd` module.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Strata - High-performance snapshot and streaming engine
#[derive(Parser)]
#[command(name = "strata", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Top-level command categories
#[derive(Subcommand)]
pub enum Commands {
    /// Data operations (pack, inspect, diff)
    #[command(subcommand)]
    Data(DataCommands),

    /// Virtual machine operations (boot, install, snap, commit)
    #[command(subcommand)]
    Vm(VmCommands),

    /// System utilities (doctor, bench, serve, keygen)
    #[command(subcommand)]
    Sys(SysCommands),
}

#[derive(Subcommand)]
pub enum DataCommands {
    /// Pack data into a Strata archive
    Pack {
        /// Path to disk image to pack
        #[arg(long)]
        disk: Option<PathBuf>,

        /// Path to memory dump to pack
        #[arg(long)]
        memory: Option<PathBuf>,

        /// Output archive path (.st)
        #[arg(short, long)]
        output: PathBuf,

        /// Compression algorithm (lz4, zstd, none)
        #[arg(long, default_value = "lz4")]
        compression: String,

        /// Enable encryption
        #[arg(long)]
        encrypt: bool,

        /// Train compression dictionary
        #[arg(long)]
        train_dict: bool,

        /// Block size in bytes
        #[arg(long, default_value_t = 65536)]
        block_size: u32,

        /// Enable content-defined chunking (CDC)
        #[arg(long)]
        cdc: bool,

        /// Minimum chunk size for CDC
        #[arg(long, default_value_t = 16384)]
        min_chunk: u32,

        /// Average chunk size for CDC
        #[arg(long, default_value_t = 65536)]
        avg_chunk: u32,

        /// Maximum chunk size for CDC
        #[arg(long, default_value_t = 131072)]
        max_chunk: u32,

        /// Suppress all output and progress bars
        #[arg(long, short)]
        silent: bool,
    },

    /// Inspect archive metadata
    Info {
        /// Path to archive
        snap: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show differences in overlay
    #[cfg(feature = "diagnostics")]
    Diff {
        /// Path to overlay
        overlay: PathBuf,

        /// Show block-level differences
        #[arg(long)]
        blocks: bool,

        /// Show file-level differences
        #[arg(long)]
        files: bool,
    },

    /// Build archive from source directory
    Build {
        /// Source directory
        #[arg(long)]
        source: PathBuf,

        /// Optional memory dump
        #[arg(long)]
        memory: Option<PathBuf>,

        /// Output archive path
        #[arg(short, long)]
        output: PathBuf,

        /// Build profile
        #[arg(long)]
        profile: Option<String>,

        /// Enable encryption
        #[arg(long)]
        encrypt: bool,

        /// Enable CDC
        #[arg(long)]
        cdc: bool,
    },

    /// Analyze archive structure
    #[cfg(feature = "diagnostics")]
    Analyze {
        /// Archive to analyze
        input: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum VmCommands {
    /// Boot a virtual machine from snapshot
    #[cfg(feature = "fuse")]
    Boot {
        /// Snapshot to boot from
        snap: String,

        /// RAM size (e.g., "4G")
        #[arg(long)]
        ram: Option<String>,

        /// Disable KVM acceleration
        #[arg(long)]
        no_kvm: bool,

        /// Network mode (user, bridge, none)
        #[arg(long, default_value = "user")]
        network: String,

        /// Hypervisor backend (qemu, firecracker)
        #[arg(long, default_value = "qemu")]
        backend: String,

        /// Persistent overlay path
        #[arg(long)]
        persist: Option<PathBuf>,

        /// QMP socket path for control
        #[arg(long)]
        qmp_socket: Option<PathBuf>,

        /// Disable graphics (headless mode)
        #[arg(long)]
        no_graphics: bool,

        /// Enable VNC server
        #[arg(long)]
        vnc: bool,
    },

    /// Install OS from ISO to snapshot
    #[cfg(feature = "fuse")]
    Install {
        /// Path to ISO image
        #[arg(long)]
        iso: PathBuf,

        /// Virtual disk size (e.g., "10G")
        #[arg(long, default_value = "10G")]
        disk_size: String,

        /// RAM size (e.g., "4G")
        #[arg(long, default_value = "4G")]
        ram: String,

        /// Output snapshot path
        #[arg(short, long)]
        output: PathBuf,

        /// Disable graphics
        #[arg(long)]
        no_graphics: bool,

        /// Enable VNC
        #[arg(long)]
        vnc: bool,

        /// Enable CDC
        #[arg(long)]
        cdc: bool,
    },

    /// Create snapshot via QMP
    Snap {
        /// QMP socket path
        #[arg(long)]
        socket: PathBuf,

        /// Base snapshot
        #[arg(long)]
        base: PathBuf,

        /// Overlay path
        #[arg(long)]
        overlay: PathBuf,

        /// Output snapshot
        #[arg(short, long)]
        output: PathBuf,
    },

    /// Commit overlay changes to new snapshot
    Commit {
        /// Base snapshot
        base: PathBuf,

        /// Overlay with changes
        overlay: PathBuf,

        /// Output snapshot
        output: PathBuf,

        /// Compression algorithm
        #[arg(long, default_value = "lz4")]
        compression: String,

        /// Block size
        #[arg(long, default_value_t = 65536)]
        block_size: u32,

        /// Keep overlay file after commit
        #[arg(long)]
        keep_overlay: bool,

        /// Flatten all layers into single archive
        #[arg(long)]
        flatten: bool,

        /// Commit message
        #[arg(long)]
        message: Option<String>,

        /// Create thin snapshot (reference base)
        #[arg(long)]
        thin: bool,
    },

    /// Mount snapshot as filesystem
    #[cfg(feature = "fuse")]
    Mount {
        /// Snapshot to mount
        snap: String,

        /// Mount point directory
        mountpoint: PathBuf,

        /// Overlay for writes
        #[arg(long)]
        overlay: Option<PathBuf>,

        /// Run as daemon
        #[arg(short, long)]
        daemon: bool,

        /// Enable read-write mode
        #[arg(long)]
        rw: bool,

        /// Cache size (e.g., "1G")
        #[arg(long)]
        cache_size: Option<String>,

        /// User ID for files
        #[arg(long, default_value_t = 1000)]
        uid: u32,

        /// Group ID for files
        #[arg(long, default_value_t = 1000)]
        gid: u32,

        /// Export as NBD device
        #[arg(long)]
        nbd: bool,
    },

    /// Unmount filesystem
    #[cfg(feature = "fuse")]
    Unmount {
        /// Mount point to unmount
        mountpoint: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum SysCommands {
    /// Run system diagnostics
    #[cfg(feature = "diagnostics")]
    Doctor,

    /// Benchmark archive performance
    #[cfg(feature = "diagnostics")]
    Bench {
        /// Archive to benchmark
        image: PathBuf,

        /// Block size for testing
        #[arg(long)]
        block_size: Option<u32>,

        /// Duration in seconds
        #[arg(long)]
        duration: Option<u64>,

        /// Number of threads
        #[arg(long)]
        threads: Option<usize>,
    },

    /// Serve archive over network
    #[cfg(feature = "server")]
    Serve {
        /// Snapshot to serve
        snap: String,

        /// Server port
        #[arg(long, default_value_t = 8080)]
        port: u16,

        /// Run as daemon
        #[arg(short, long)]
        daemon: bool,

        /// Enable NBD protocol
        #[arg(long)]
        nbd: bool,

        /// Enable S3-compatible API
        #[arg(long)]
        s3: bool,
    },

    /// Generate signing keys
    #[cfg(feature = "signing")]
    Keygen {
        /// Output directory for keys
        #[arg(short, long)]
        output_dir: Option<PathBuf>,
    },

    /// Sign archive
    #[cfg(feature = "signing")]
    Sign {
        /// Private key path
        #[arg(long)]
        key: PathBuf,

        /// Archive to sign
        image: PathBuf,
    },

    /// Verify archive signature
    #[cfg(feature = "signing")]
    Verify {
        /// Public key path
        #[arg(long)]
        key: PathBuf,

        /// Archive to verify
        image: PathBuf,
    },
}
