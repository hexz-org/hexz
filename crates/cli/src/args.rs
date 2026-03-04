use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// High-performance, deduplicated data archives
#[derive(Parser)]
#[command(name = "hexz", version, about = "High-performance, deduplicated data archives", long_about = None)]
#[command(disable_help_flag = true)] 
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

#[derive(Subcommand, Debug, Clone)]
pub enum RemoteCommand {
    /// Add a new remote
    Add {
        name: String,
        url: String,
    },
    /// Remove a remote
    Remove {
        name: String,
    },
    /// List all remotes
    List,
}

#[derive(Subcommand)]
pub enum Commands {
    // ------------------------------------------------------------------------
    // Archive Operations
    // ------------------------------------------------------------------------
    /// Pack a file or directory into a deduplicated archive
    #[command(display_order = 1)]
    #[command(
        long_about = "Creates a compressed and deduplicated archive (.hxz). Uses Content-Defined Chunking (CDC) to identify shared blocks across versions."
    )]
    #[command(after_help = "hexz pack ./folder data.hxz --compression zstd")]
    Pack {
        /// Path to input data
        input: PathBuf,

        /// Output archive path (.hxz)
        output: PathBuf,

        /// Base archive to diff against
        #[arg(long, short)]
        base: Option<PathBuf>,

        /// Compression algorithm (lz4, zstd, none)
        #[arg(long, default_value = "lz4")]
        compression: String,

        /// Enable encryption
        #[arg(long)]
        encrypt: bool,

        /// Block size in bytes
        #[arg(long, default_value_t = 65536, value_parser = clap::value_parser!(u32).range(1..))]
        block_size: u32,

        /// Number of compression worker threads (0 = auto)
        #[arg(long)]
        workers: Option<usize>,

        /// Run adaptive CDC analysis
        #[arg(long)]
        dcam: bool,

        /// Run extensive DCAM analysis to find globally optimal parameters (up to 16MB chunks)
        #[arg(long)]
        dcam_optimal: bool,

        /// Suppress output
        #[arg(long, short)]
        silent: bool,
    },

    /// Extract an archive back to a file or directory
    #[command(display_order = 2)]
    #[command(after_help = "hexz extract data.hxz ./output")]
    Extract {
        /// Input .hxz archive
        input: PathBuf,

        /// Output path
        output: PathBuf,
    },

    /// Show archive details and metadata
    #[command(display_order = 3)]
    #[command(alias = "inspect")]
    #[command(after_help = "hexz show ./data.hxz --json")]
    Show {
        /// Path to archive
        snap: PathBuf,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Compare two archives and show storage savings
    #[command(display_order = 4)]
    #[command(after_help = "hexz diff v1.hxz v2.hxz")]
    Diff {
        /// First archive
        a: PathBuf,

        /// Second archive
        b: PathBuf,
    },

    /// List archives in a directory and show their lineage
    #[command(display_order = 5)]
    #[command(alias = "ls")]
    Log {
        /// Directory to scan
        dir: PathBuf,
    },

    /// Convert external data formats into Hexz archives
    #[command(display_order = 6)]
    Convert {
        /// Input format (tar, hdf5, webdataset)
        format: String,

        /// Input path
        input: PathBuf,

        /// Output archive path
        output: PathBuf,

        /// Compression algorithm
        #[arg(long, default_value = "lz4")]
        compression: String,

        /// Block size
        #[arg(long, default_value_t = 65536)]
        block_size: u32,

        /// Profile name
        #[arg(short, long)]
        profile: Option<String>,

        /// Suppress output
        #[arg(long, short)]
        silent: bool,
    },

    /// Predict compression and deduplication potential
    #[command(display_order = 7)]
    Predict {
        /// Path to analyze
        path: PathBuf,

        /// Block size to test
        #[arg(long, default_value_t = 65536)]
        block_size: u32,

        /// Minimum chunk size for CDC
        #[arg(long)]
        min_chunk: Option<u32>,

        /// Average chunk size for CDC
        #[arg(long)]
        avg_chunk: Option<u32>,

        /// Maximum chunk size for CDC
        #[arg(long)]
        max_chunk: Option<u32>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    // ------------------------------------------------------------------------
    // Filesystem Operations
    // ------------------------------------------------------------------------
    /// Mount an archive as a FUSE filesystem
    #[cfg(feature = "fuse")]
    #[command(display_order = 10)]
    #[command(
        long_about = "Exposes the archive's content as a read-only filesystem. Only requested blocks are fetched/decompressed on-demand."
    )]
    #[command(after_help = "hexz mount data.hxz /mnt/data")]
    Mount {
        /// Archive to mount
        snap: String,

        /// Mount point directory
        mountpoint: PathBuf,

        /// Optional write layer directory for modifications
        #[arg(long, short)]
        overlay: Option<PathBuf>,

        /// Make the mount writable by automatically using a temporary overlay
        #[arg(long, short = 'e')]
        editable: bool,

        /// Run as daemon
        #[arg(short, long)]
        daemon: bool,

        /// Cache size (e.g., "1G")
        #[arg(long)]
        cache_size: Option<String>,

        /// User ID for files
        #[arg(long, default_value_t = 0)]
        uid: u32,

        /// Group ID for files
        #[arg(long, default_value_t = 0)]
        gid: u32,
    },

    /// Unmount a previously mounted archive
    #[cfg(feature = "fuse")]
    #[command(display_order = 11)]
    Unmount {
        /// Mount point to unmount
        mountpoint: PathBuf,
    },

    /// Drop into a shell within a mounted archive
    #[cfg(feature = "fuse")]
    #[command(display_order = 11)]
    #[command(
        long_about = "Mounts the archive to a temporary directory and drops you into a subshell. When the shell exits, the archive is automatically unmounted and the temporary directory is cleaned up."
    )]
    #[command(after_help = "hexz shell data.hxz --editable")]
    Shell {
        /// Archive to mount
        snap: String,

        /// Optional write layer directory for modifications
        #[arg(long, short)]
        overlay: Option<PathBuf>,

        /// Make the mount writable by automatically using a temporary overlay
        #[arg(long, short = 'e')]
        editable: bool,

        /// Cache size (e.g., "1G")
        #[arg(long)]
        cache_size: Option<String>,
    },

    /// Commit changes from a writable mount to a new thin archive
    #[cfg(feature = "fuse")]
    #[command(display_order = 12)]
    #[command(
        long_about = "Takes a writable mount point and saves the modifications as a new thin archive linked to the original base."
    )]
    #[command(after_help = "hexz commit v2.hxz")]
    Commit {
        /// Output archive path (.hxz)
        output: PathBuf,

        /// Mount point directory or workspace path (defaults to current directory)
        mountpoint: Option<PathBuf>,

        /// Base archive to link against (optional if can be inferred)
        #[arg(long, short)]
        base: Option<PathBuf>,
    },

    /// Initialize a workspace from an archive (Git-style)
    #[cfg(feature = "fuse")]
    #[command(display_order = 13)]
    #[command(alias = "co")]
    Checkout {
        /// Archive to use as base
        archive: PathBuf,

        /// Directory to create the workspace in
        path: PathBuf,
    },

    /// Show changes in the current workspace
    #[cfg(feature = "fuse")]
    #[command(display_order = 14)]
    #[command(alias = "st")]
    Status {
        /// Workspace path (defaults to current directory)
        path: Option<PathBuf>,
    },

    /// Initialize a new empty workspace
    #[cfg(feature = "fuse")]
    #[command(display_order = 15)]
    Init {
        /// Directory to create the workspace in (defaults to current directory)
        path: Option<PathBuf>,
    },

    /// Manage remote endpoints for the workspace
    #[cfg(feature = "fuse")]
    #[command(display_order = 16)]
    Remote {
        #[command(subcommand)]
        action: RemoteCommand,
    },

    /// Push thin archives to a remote endpoint
    #[cfg(feature = "fuse")]
    #[command(display_order = 17)]
    Push {
        /// Remote name (defaults to "origin")
        #[arg(default_value = "origin")]
        remote: String,
        
        /// Archive to push (defaults to the workspace's base archive)
        archive: Option<PathBuf>,
    },

    /// Pull thin archives from a remote endpoint
    #[cfg(feature = "fuse")]
    #[command(display_order = 18)]
    Pull {
        /// Remote name (defaults to "origin")
        #[arg(default_value = "origin")]
        remote: String,
    },

    // ------------------------------------------------------------------------
    // Networking & Security
    // ------------------------------------------------------------------------
    /// Serve an archive over the network (HTTP range requests)
    #[cfg(feature = "server")]
    #[command(display_order = 20)]
    Serve {
        /// Archive to serve
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
    },

    /// Generate an Ed25519 signing keypair
    #[cfg(feature = "signing")]
    #[command(display_order = 21)]
    Keygen {
        /// Output directory for keys
        #[arg(short, long)]
        output_dir: Option<PathBuf>,
    },

    /// Sign an archive
    #[cfg(feature = "signing")]
    #[command(display_order = 22)]
    Sign {
        /// Private key path
        key: PathBuf,

        /// Archive to sign
        image: PathBuf,
    },

    /// Verify an archive's signature
    #[cfg(feature = "signing")]
    #[command(display_order = 23)]
    Verify {
        /// Public key path
        key: PathBuf,

        /// Archive to verify
        image: PathBuf,
    },

    /// Run system health check and diagnostics
    #[command(display_order = 24)]
    Doctor,
}
