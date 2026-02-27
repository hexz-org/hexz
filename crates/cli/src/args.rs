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
    #[command(after_help = "hexz pack model.hxz --disk ./model.bin --compression zstd")]
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

        /// Minimum CDC chunk size (auto-detected if not specified)
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        min_chunk: Option<u32>,

        /// Average CDC chunk size (auto-detected if not specified)
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        avg_chunk: Option<u32>,

        /// Maximum CDC chunk size (auto-detected if not specified)
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        max_chunk: Option<u32>,

        /// Number of compression worker threads (0 = auto)
        #[arg(long)]
        workers: Option<usize>,

        /// Run DCAM analysis to auto-tune CDC chunk sizes (slower but adaptive).
        /// Without this flag, CDC uses global defaults: min=16 KiB, avg=64 KiB, max=256 KiB.
        #[arg(long)]
        dcam: bool,

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

    /// Compare block hashes between two archives
    #[command(display_order = 3)]
    #[command(
        long_about = "Compares the BLAKE3 block hashes of two Hexz archives.\n\nReports how much data is shared between them, unique to each, and the storage savings achieved through deduplication. Useful for understanding how much a fine-tuned checkpoint differs from its base."
    )]
    #[command(after_help = "hexz diff base.hxz finetuned.hxz")]
    Diff {
        /// First archive
        a: PathBuf,

        /// Second archive
        b: PathBuf,
    },

    /// List archives in a directory as a lineage tree
    #[command(display_order = 4)]
    #[command(
        long_about = "Scans a directory for .hxz archives and renders their parent-child relationships as a tree.\n\nParent links are read from each archive's header. Archives whose declared parent lives outside the scanned directory are annotated as external."
    )]
    #[command(after_help = "hexz ls ./checkpoints/")]
    Ls {
        /// Directory to scan
        dir: PathBuf,
    },

    /// Pack with profile-based presets
    #[command(display_order = 4)]
    #[command(
        long_about = "Creates a Hexz archive using a named build profile.\n\nProfiles automatically select compression, block size, and dictionary training settings optimized for different workloads (ML, EDA, embedded, generic)."
    )]
    #[command(after_help = "hexz build disk.img archive.hxz --profile ml")]
    Build {
        /// Source disk image
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

        /// Build profile (ml, eda, embedded, generic)
        #[arg(long)]
        profile: Option<String>,

        /// Suppress output
        #[arg(long, short)]
        silent: bool,
    },

    /// Estimate space savings before packing
    #[command(display_order = 7)]
    #[command(
        long_about = "Quickly estimates the compression and deduplication savings if a raw data file\nwere packed into the Hexz format. Samples blocks without reading the whole file,\nso it completes in seconds even on multi-GB inputs."
    )]
    #[command(after_help = "hexz predict model.bin --block-size 65536 --json")]
    Predict {
        /// Path to the raw data file to analyze
        file: PathBuf,

        /// Block size in bytes
        #[arg(long, default_value_t = 65536)]
        block_size: u32,

        /// Minimum CDC chunk size (auto-detected if not specified)
        #[arg(long)]
        min_chunk: Option<u32>,

        /// Average CDC chunk size (auto-detected if not specified)
        #[arg(long)]
        avg_chunk: Option<u32>,

        /// Maximum CDC chunk size (auto-detected if not specified)
        #[arg(long)]
        max_chunk: Option<u32>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
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

        /// Path to memory dump to include
        #[arg(long)]
        memory: Option<PathBuf>,

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

        /// Prefetch window size (number of blocks to read ahead)
        #[arg(long)]
        prefetch: Option<u32>,
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
        long_about = "Checks the system for compatibility with Hexz features (FUSE, QEMU, network)."
    )]
    #[command(after_help = "hexz doctor")]
    Doctor,

    /// Serve archive over network
    #[cfg(feature = "server")]
    #[command(display_order = 22)]
    #[command(
        long_about = "Starts an HTTP server to stream the snapshot over the network.\n\nClients can fetch specific byte ranges efficiently."
    )]
    #[command(after_help = "hexz serve model.hxz --port 8080")]
    Serve {
        /// Snapshot to serve
        snap: String,

        /// Server port
        #[arg(long, default_value_t = 8080)]
        port: u16,

        /// Bind address
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,

        /// Run as daemon
        #[arg(short, long)]
        daemon: bool,

        /// Enable NBD protocol
        #[arg(long)]
        nbd: bool,
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
