use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Hexz - High-performance snapshot and streaming engine
#[derive(Parser)]
#[command(name = "hexz", version, about, long_about = None)]
#[command(disable_help_flag = true)] // We handle help manually
#[command(styles = get_styles())]
pub struct Cli {
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    pub help: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

fn get_styles() -> clap::builder::Styles {
    use clap::builder::styling::{AnsiColor, Effects, Styles};
    Styles::styled()
        .header(AnsiColor::Yellow.on_default() | Effects::BOLD)
        .usage(AnsiColor::Green.on_default() | Effects::BOLD)
        .literal(AnsiColor::Cyan.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Cyan.on_default())
}

/// Top-level command categories
#[derive(Subcommand)]
pub enum Commands {
    // ------------------------------------------------------------------------
    // Archive Operations
    // ------------------------------------------------------------------------
    /// Pack data into a Hexz archive
    #[command(display_order = 1)]
    #[command(
        long_about = "Creates a highly compressed, encrypted, and deduplicated archive from a disk image or memory dump.\n\nIt uses Content-Defined Chunking (CDC) to ensure that only changed weights are stored when archiving multiple versions of a model. This is the primary way to ingest data into Hexz."
    )]
    #[command(after_help = "hexz pack model.hxz --disk ./model.bin --compression zstd --cdc")]
    Pack {
        /// Output archive path (.hxz)
        output: PathBuf,

        /// Path to disk image to pack
        #[arg(long)]
        disk: Option<PathBuf>,

        /// Path to memory dump to pack
        #[arg(long)]
        memory: Option<PathBuf>,

        /// Compression algorithm (lz4, zstd, none)
        #[arg(long, default_value = "lz4")]
        compression: String,

        /// Enable encryption
        #[arg(long)]
        encrypt: bool,

        /// Train compression dictionary
        #[arg(long)]
        train_dict: bool,

        /// Block size in bytes (must be > 0)
        #[arg(long, default_value_t = 65536, value_parser = clap::value_parser!(u32).range(1..))]
        block_size: u32,

        /// Enable content-defined chunking (CDC)
        #[arg(long)]
        cdc: bool,

        /// Minimum chunk size for CDC
        #[arg(long, default_value_t = 16384, value_parser = clap::value_parser!(u32).range(1..))]
        min_chunk: u32,

        /// Average chunk size for CDC
        #[arg(long, default_value_t = 65536, value_parser = clap::value_parser!(u32).range(1..))]
        avg_chunk: u32,

        /// Maximum chunk size for CDC
        #[arg(long, default_value_t = 131072, value_parser = clap::value_parser!(u32).range(1..))]
        max_chunk: u32,

        /// Suppress all output and progress bars
        #[arg(long, short)]
        silent: bool,
    },

    /// Inspect archive metadata
    #[command(display_order = 2)]
    #[command(
        long_about = "Reads the header and index of a Hexz archive without decompressing the full body.\n\nUse this to verify archive integrity, check compression ratios, or view metadata about the stored snapshot."
    )]
    #[command(after_help = "hexz inspect ./model.hxz --json")]
    Inspect {
        /// Path to archive
        snap: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show differences in overlay
    #[cfg(feature = "diagnostics")]
    #[command(display_order = 3)]
    #[command(
        long_about = "Analyzes the differences between a base image and an overlay.\n\nThis is useful for auditing what changed in a fine-tuning run or verifying that a thin snapshot only contains the expected deltas."
    )]
    #[command(after_help = "hexz diff finetuned.overlay --blocks")]
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
    #[command(display_order = 4)]
    #[command(
        long_about = "Recursively builds a Hexz archive from a local directory structure.\n\nUnlike 'pack' which handles raw disk images, 'build' is designed for file-system level packing."
    )]
    #[command(after_help = "hexz build ./checkpoints/ archive.hxz")]
    Build {
        /// Source directory
        source: PathBuf,

        /// Output archive path
        output: PathBuf,

        /// Optional memory dump
        #[arg(long)]
        memory: Option<PathBuf>,

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
    #[command(display_order = 5)]
    #[command(
        long_about = "Performs a deep structural analysis of the archive format.\n\nUsed primarily for debugging corruption issues or optimizing block alignment strategies."
    )]
    #[command(after_help = "hexz analyze ./corrupt_image.hxz")]
    Analyze {
        /// Archive to analyze
        input: PathBuf,
    },

    /// Convert external formats to Hexz snapshot
    #[command(display_order = 6)]
    #[command(
        long_about = "Ingests external formats like tar, HDF5, or WebDataset into a Hexz snapshot.\n\nThis allows legacy datasets to benefit from Hexz's random access and deduplication features."
    )]
    #[command(after_help = "hexz convert tar data.tar data.hxz")]
    Convert {
        /// Source format (tar, hdf5, webdataset)
        format: String,

        /// Input file path
        input: PathBuf,

        /// Output snapshot path (.hxz)
        output: PathBuf,

        /// Compression algorithm (lz4, zstd)
        #[arg(long, default_value = "lz4")]
        compression: String,

        /// Block size in bytes
        #[arg(long, default_value_t = 65536)]
        block_size: u32,

        /// Build profile (ml, eda, embedded, generic, archival)
        #[arg(long)]
        profile: Option<String>,

        /// Suppress output
        #[arg(long, short)]
        silent: bool,
    },

    // ------------------------------------------------------------------------
    // Virtual Machine Operations
    // ------------------------------------------------------------------------
    /// Boot a virtual machine from snapshot
    #[cfg(feature = "fuse")]
    #[command(display_order = 10)]
    #[command(
        long_about = "Boots a transient Virtual Machine directly from a Hexz snapshot.\n\nThe VM uses a copy-on-write overlay, meaning the original snapshot remains immutable. Changes are lost on shutdown unless --persist is used."
    )]
    #[command(after_help = "hexz boot ubuntu.hxz --ram 4G --no-graphics")]
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
    #[command(display_order = 11)]
    #[command(
        long_about = "Runs an OS installer from an ISO and captures the result into a new Hexz snapshot.\n\nThis automates the process of creating base images for VMs."
    )]
    #[command(after_help = "hexz install alpine.iso alpine-base.hxz")]
    Install {
        /// Path to ISO image
        iso: PathBuf,

        /// Output snapshot path
        output: PathBuf,

        /// Virtual disk size (e.g., "10G")
        #[arg(long, default_value = "10G")]
        primary_size: String,

        /// RAM size (e.g., "4G")
        #[arg(long, default_value = "4G")]
        ram: String,

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
    #[cfg(unix)]
    #[command(display_order = 12)]
    #[command(
        long_about = "Triggers a live snapshot of a running VM via the QMP socket.\n\nThis allows for capturing the state of a running system without shutting it down."
    )]
    #[command(after_help = "hexz snap /tmp/qmp.sock base.hxz overlay.bin live.hxz")]
    Snap {
        /// QMP socket path
        socket: PathBuf,

        /// Base snapshot
        base: PathBuf,

        /// Overlay path
        overlay: PathBuf,

        /// Output snapshot
        output: PathBuf,
    },

    /// Commit overlay changes to new snapshot
    #[command(display_order = 13)]
    #[command(
        long_about = "Finalizes a writable overlay into a new immutable snapshot.\n\nSupports 'thin' snapshots which only store the deltas referencing the parent, ideal for iterative model fine-tuning."
    )]
    #[command(after_help = "hexz commit base.hxz overlay.bin new_model.hxz --thin")]
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

        /// Block size (must be > 0)
        #[arg(long, default_value_t = 65536, value_parser = clap::value_parser!(u32).range(1..))]
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
    #[command(display_order = 14)]
    #[command(
        long_about = "Mounts a Hexz snapshot as a FUSE filesystem.\n\nAllows standard tools to read data from the snapshot as if it were a normal directory."
    )]
    #[command(after_help = "hexz mount model.hxz /mnt/model --rw")]
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
    #[command(display_order = 15)]
    #[command(long_about = "Unmounts a previously mounted Hexz filesystem.")]
    #[command(after_help = "hexz unmount /mnt/model")]
    Unmount {
        /// Mount point to unmount
        mountpoint: PathBuf,
    },

    // ------------------------------------------------------------------------
    // System & Diagnostics
    // ------------------------------------------------------------------------
    /// Run system diagnostics
    #[cfg(feature = "diagnostics")]
    #[command(display_order = 20)]
    #[command(
        long_about = "Checks the system for compatibility with Hexz features (FUSE, KVM, AVX2, etc.)."
    )]
    #[command(after_help = "hexz doctor")]
    Doctor,

    /// Benchmark archive performance
    #[cfg(feature = "diagnostics")]
    #[command(display_order = 21)]
    #[command(
        long_about = "Runs read/write benchmarks on a specific archive to test throughput and latency."
    )]
    #[command(after_help = "hexz bench model.hxz --threads 4")]
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
    #[command(display_order = 22)]
    #[command(
        long_about = "Starts an HTTP/S3 compatible server to stream the snapshot over the network.\n\nClients can fetch specific byte ranges efficiently."
    )]
    #[command(after_help = "hexz serve model.hxz --port 8080")]
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
    #[command(display_order = 23)]
    #[command(long_about = "Generates an Ed25519 keypair for signing Hexz archives.")]
    #[command(after_help = "hexz keygen --output-dir ~/.hexz/keys")]
    Keygen {
        /// Output directory for keys
        #[arg(short, long)]
        output_dir: Option<PathBuf>,
    },

    /// Sign archive
    #[cfg(feature = "signing")]
    #[command(display_order = 24)]
    #[command(long_about = "Cryptographically signs a Hexz archive using a private key.")]
    #[command(after_help = "hexz sign private.pem model.hxz")]
    Sign {
        /// Private key path
        key: PathBuf,

        /// Archive to sign
        image: PathBuf,
    },

    /// Verify archive signature
    #[cfg(feature = "signing")]
    #[command(display_order = 25)]
    #[command(
        long_about = "Verifies the cryptographic signature of an archive using a public key."
    )]
    #[command(after_help = "hexz verify public.pem model.hxz")]
    Verify {
        /// Public key path
        key: PathBuf,

        /// Archive to verify
        image: PathBuf,
    },
}
