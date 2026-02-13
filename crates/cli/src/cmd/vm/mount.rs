//! Mount Strata snapshots as FUSE filesystems or NBD block devices.
//!
//! This command exposes Strata snapshots to the host operating system by mounting
//! them as either FUSE filesystems (default) or NBD (Network Block Device) devices.
//! The mount provides read-only or read-write access with optional overlay support
//! for copy-on-write semantics, cache configuration, and permission mapping.
//!
//! # Mount Mechanisms
//!
//! ## FUSE (Filesystem in Userspace) - Default
//!
//! **Characteristics:**
//! - No root/sudo required (uses user-space FUSE)
//! - Exposes `disk` and `memory` files at mount point
//! - Supports overlay for read-write mounts
//! - Can run as daemon for background mounting
//!
//! **File Structure:**
//! ```text
//! /mnt/snapshot/
//! ├── disk       # Disk image (raw format)
//! └── memory     # Memory dump (if present in snapshot)
//! ```
//!
//! **Use Cases:**
//! - Mount disk images for file extraction without VM
//! - Read-write mounts with overlay for ephemeral changes
//! - Development and debugging (inspect snapshot contents)
//! - Backup and archival access
//!
//! ## NBD (Network Block Device) - Optional
//!
//! **Characteristics:**
//! - Requires root/sudo and `nbd` kernel module
//! - Creates `/dev/nbdN` block device
//! - Supports standard filesystem mounting (ext4, xfs, etc.)
//! - Better performance for large sequential I/O
//!
//! **Workflow:**
//! 1. Start internal NBD server on localhost
//! 2. Connect `nbd-client` to server
//! 3. Mount resulting `/dev/nbdN` device
//! 4. Wait for Ctrl+C, then cleanup
//!
//! **Use Cases:**
//! - Native filesystem mounting without FUSE overhead
//! - Integration with existing block device tooling
//! - Performance-critical applications
//!
//! # Overlay Behavior
//!
//! When mounted read-write (`--rw`), an overlay file tracks modifications:
//!
//! **Overlay Mechanism:**
//! - **Reads**: Served from base snapshot (fast, cached)
//! - **Writes**: Captured in overlay file at 4 KiB granularity
//! - **Metadata**: `.meta` file tracks modified block indices
//!
//! **Overlay Storage:**
//! - Ephemeral (default): Temporary file deleted on unmount
//! - Persistent (`--overlay <path>`): Saved for later commit
//!
//! **Commit Workflow:**
//! ```bash
//! # Mount with persistent overlay
//! strata mount snapshot.st /mnt --rw --overlay changes.overlay
//!
//! # Make changes inside /mnt
//! # ...
//!
//! # Unmount
//! strata unmount /mnt
//!
//! # Commit changes to new snapshot
//! strata vm commit --base snapshot.st --overlay changes.overlay --output new.st
//! ```
//!
//! # Cache Size Semantics
//!
//! The `--cache-size` parameter controls in-memory block caching:
//!
//! **Cache Behavior:**
//! - Stores recently accessed compressed blocks in memory
//! - LRU (Least Recently Used) eviction policy
//! - Reduces decompression overhead for repeated reads
//!
//! **Size Guidelines:**
//! - Default: No cache (every read decompresses from storage)
//! - `--cache-size 256M`: 256 MB cache (good for development VMs)
//! - `--cache-size 1G`: 1 GB cache (good for production workloads)
//! - `--cache-size 4G`: 4 GB cache (maximum benefit for most workloads)
//!
//! **Performance Impact:**
//! - Cache hit: ~10 GB/s (memcpy from cache)
//! - Cache miss: ~500 MB/s (LZ4) or ~200 MB/s (Zstd)
//! - Working set > cache size: Performance degrades to uncached speed
//!
//! # UID/GID Implications
//!
//! The `--uid` and `--gid` parameters control file ownership inside the mount:
//!
//! **Ownership Mapping:**
//! - All files inside the mount appear owned by `uid:gid`
//! - Does not modify actual data in snapshot (metadata-only)
//! - Affects permission checks for file access
//!
//! **Common Values:**
//! - `--uid 1000 --gid 1000`: Default user (typical for desktop Linux)
//! - `--uid 0 --gid 0`: Root ownership (for system images)
//! - Custom values: Match specific user requirements
//!
//! **Use Cases:**
//! - Allow non-root users to access mounted disk images
//! - Match ownership to container or VM user mappings
//! - Control write permissions in read-write mounts
//!
//! # FUSE Integration Details
//!
//! The FUSE implementation uses the `fuser` crate to implement:
//!
//! **FUSE Operations:**
//! - `lookup()`: Resolves `disk` and `memory` file entries
//! - `getattr()`: Returns file metadata (size, permissions, ownership)
//! - `read()`: Reads data from snapshot at specified offset
//! - `write()`: Writes data to overlay (read-write mode only)
//! - `release()`: Syncs overlay metadata on file close
//!
//! **Mount Options:**
//! - `FSName=strata`: Identifies mount in `/proc/mounts`
//! - `DefaultPermissions`: Enables kernel permission checks
//! - `RO` or `RW`: Read-only or read-write mode
//!
//! # Common Usage Patterns
//!
//! ```bash
//! # Read-only FUSE mount
//! strata mount snapshot.st /mnt
//!
//! # Read-write mount with ephemeral overlay
//! strata mount snapshot.st /mnt --rw
//!
//! # Read-write mount with persistent overlay
//! strata mount snapshot.st /mnt --rw --overlay changes.overlay
//!
//! # Mount as daemon with cache
//! strata mount snapshot.st /mnt --daemon --cache-size 512M
//!
//! # NBD mount (requires root)
//! sudo strata mount snapshot.st /mnt --nbd
//!
//! # Custom ownership
//! strata mount snapshot.st /mnt --uid 1000 --gid 1000
//! ```

use anyhow::{Context, Result};
use daemonize::Daemonize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use strata_common::constants::DEFAULT_ZSTD_LEVEL;
use strata_core::StrataFile;
use strata_core::algo::compression::{Compressor, lz4::Lz4Compressor, zstd::ZstdCompressor};
use strata_core::algo::encryption::aes_gcm::AesGcmEncryptor;
use strata_core::format::header::{CompressionType, StrataHeader};
use strata_core::format::magic::HEADER_SIZE;
use strata_core::store::StorageBackend;
use strata_core::store::local::FileBackend;

/// Parses human-readable size strings into byte counts.
///
/// Supports common size suffixes: K/KB, M/MB, G/GB, T/TB (case-insensitive).
/// Numbers can be integers or floating-point values.
///
/// # Arguments
///
/// * `s` - Size string (e.g., "256M", "1.5G", "512KB", "1024")
///
/// # Returns
///
/// Size in bytes as `usize`.
///
/// # Examples
///
/// ```text
/// "256M"  → 268435456
/// "1.5G"  → 1610612736
/// "512KB" → 524288
/// "1024"  → 1024
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - The numeric part cannot be parsed as `f64`
/// - The suffix is not recognized (valid: k, kb, m, mb, g, gb, t, tb, or empty)
pub(crate) fn parse_size(s: &str) -> Result<usize> {
    let s = s.trim();
    let (num, suffix) = if let Some(idx) = s.find(|c: char| !c.is_numeric() && c != '.') {
        (&s[..idx], &s[idx..])
    } else {
        (s, "")
    };

    let n: f64 = num.parse()?;
    let multiplier = match suffix.to_lowercase().as_str() {
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        "" => 1.0,
        _ => return Err(anyhow::anyhow!("Invalid size suffix: {}", suffix)),
    };

    Ok((n * multiplier) as usize)
}

/// Opens a Strata snapshot and initializes decompression and decryption.
///
/// This helper function is shared by both FUSE and NBD code paths. It handles:
/// - Reading and parsing the snapshot header
/// - Prompting for password if snapshot is encrypted
/// - Loading compression dictionary if present
/// - Initializing the appropriate decompressor (LZ4 or Zstd)
/// - Setting up decryption if required
/// - Configuring block cache if cache size is specified
///
/// # Arguments
///
/// * `strata_path` - Path to the `.st` snapshot file (can be relative or absolute)
/// * `cache_size` - Optional cache size string (e.g., "256M", "1G")
///
/// # Returns
///
/// `Arc<StrataFile>` that can be shared across threads for concurrent access.
///
/// # Password Handling
///
/// If the snapshot header indicates encryption:
/// - Prompts user for password via `rpassword::prompt_password()`
/// - Derives encryption key using PBKDF2 with salt from header
/// - Initializes AES-256-GCM decryptor
///
/// # Errors
///
/// Returns an error if:
/// - Snapshot file cannot be opened or read
/// - Header deserialization fails (corrupted snapshot)
/// - Password is incorrect (decryption fails)
/// - Dictionary cannot be loaded (corrupted dictionary region)
/// - Cache size string is malformed
fn open_snapshot(strata_path: &str, cache_size: Option<String>) -> Result<Arc<StrataFile>> {
    let abs_strata_path = std::fs::canonicalize(strata_path)
        .context(format!("Failed to resolve snapshot path: {}", strata_path))?;

    // Pre-read header for password prompt
    let (header, password) = {
        let backend = FileBackend::new(&abs_strata_path)?;
        let header_bytes = backend.read_exact(0, HEADER_SIZE)?;
        let header: StrataHeader = bincode::deserialize(&header_bytes)?;

        let password = if header.encryption.is_some() {
            Some(rpassword::prompt_password("Enter encryption password: ")?)
        } else {
            None
        };
        (header, password)
    };

    let backend = Arc::new(FileBackend::new(&abs_strata_path)?);

    let dictionary = if let (Some(offset), Some(length)) =
        (header.dictionary_offset, header.dictionary_length)
    {
        Some(backend.read_exact(offset, length as usize)?.to_vec())
    } else {
        None
    };

    let compressor: Box<dyn Compressor> = match header.compression {
        CompressionType::Lz4 => Box::new(Lz4Compressor::new()),
        CompressionType::Zstd => Box::new(ZstdCompressor::new(DEFAULT_ZSTD_LEVEL, dictionary)),
    };

    let encryptor = if let (Some(params), Some(pass)) = (header.encryption, password) {
        Some(Box::new(AesGcmEncryptor::new(
            pass.as_bytes(),
            &params.salt,
            params.iterations,
        ))
            as Box<dyn strata_core::algo::encryption::Encryptor>)
    } else {
        None
    };

    let cache_capacity = if let Some(s) = cache_size {
        Some(parse_size(&s)?)
    } else {
        None
    };

    Ok(StrataFile::with_cache(
        backend,
        compressor,
        encryptor,
        cache_capacity,
        None, // No prefetching for mount command
    )?)
}

/// Executes the mount command to expose a snapshot via FUSE or NBD.
///
/// Mounts a Strata snapshot at the specified mountpoint using either FUSE
/// (default) or NBD (if `--nbd` flag is set). Supports read-only and read-write
/// modes, optional overlay for copy-on-write, caching, and daemon mode.
///
/// # Arguments
///
/// * `strata_path` - Path to the `.st` snapshot file
/// * `mountpoint` - Directory where snapshot will be mounted
/// * `overlay` - Optional overlay file path for read-write mounts (persistent)
/// * `daemon` - If true, daemonize the process and run in background
/// * `rw` - If true, mount read-write with overlay; otherwise read-only
/// * `cache_size` - Optional cache size (e.g., "256M", "1G")
/// * `uid` - User ID for file ownership inside mount
/// * `gid` - Group ID for file ownership inside mount
/// * `nbd` - If true, use NBD instead of FUSE (requires root)
///
/// # FUSE Mode (Default)
///
/// Creates a FUSE filesystem at `mountpoint` with:
/// - `disk` file containing disk image
/// - `memory` file containing memory dump (if present in snapshot)
///
/// **Read-Only Mode:**
/// - All writes are rejected with EROFS error
/// - No overlay file created
/// - Safe for concurrent mounts of same snapshot
///
/// **Read-Write Mode:**
/// - Writes captured in overlay file (4 KiB granularity)
/// - Metadata tracked in `.meta` file
/// - Overlay can be persistent (specified path) or ephemeral (temp file)
///
/// **Daemon Mode:**
/// - Process detaches and runs in background
/// - Logs redirected to `/tmp/strata.log` and `/tmp/strata.err`
/// - Working directory changed to `/`
/// - Useful for long-running mounts
///
/// # NBD Mode (`--nbd`)
///
/// Creates an NBD block device and mounts it:
/// 1. Starts internal NBD server on ephemeral port (localhost only)
/// 2. Runs `nbd-client` to connect to server and create `/dev/nbdN`
/// 3. Runs `mount` to mount the NBD device at `mountpoint`
/// 4. Waits for Ctrl+C
/// 5. Unmounts and cleans up NBD device
///
/// **Requirements:**
/// - Root privileges (uses `sudo` for `nbd-client` and `mount`)
/// - `nbd` kernel module loaded (`sudo modprobe nbd`)
/// - `nbd-client` utility installed
///
/// **NBD Server:**
/// - Binds to `127.0.0.1` (localhost only, not exposed to network)
/// - Automatically selects free ephemeral port
/// - Serves disk stream only (memory not exposed via NBD)
///
/// # Overlay File Paths
///
/// If `overlay` is specified:
/// - Absolute path: Used as-is
/// - Relative path: Resolved relative to current working directory
/// - Path does not need to exist (will be created)
///
/// If `overlay` is None and `rw` is true:
/// - Creates temporary file that is deleted on unmount
/// - Useful for ephemeral changes (testing, development)
///
/// # Errors
///
/// Returns an error if:
/// - Snapshot file cannot be opened
/// - Mountpoint does not exist or is not a directory
/// - FUSE mount fails (permissions, already mounted)
/// - NBD server fails to start (port unavailable, feature not compiled)
/// - NBD client/mount commands fail (not installed, wrong permissions)
/// - Daemonization fails (resource limits, permissions)
///
/// # Examples
///
/// ```no_run
/// use std::path::PathBuf;
/// use strata_cli::cmd::vm::mount;
///
/// // Read-only FUSE mount
/// mount::run(
///     "snapshot.st".to_string(),
///     PathBuf::from("/mnt"),
///     None,
///     false, // not daemon
///     false, // read-only
///     None,  // no cache
///     1000,  // uid
///     1000,  // gid
///     false, // FUSE mode
/// )?;
///
/// // Read-write mount with persistent overlay
/// mount::run(
///     "snapshot.st".to_string(),
///     PathBuf::from("/mnt"),
///     Some(PathBuf::from("changes.overlay")),
///     false,
///     true,  // read-write
///     Some("512M".to_string()), // 512 MB cache
///     1000,
///     1000,
///     false,
/// )?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[allow(clippy::too_many_arguments)]
pub fn run(
    strata_path: String,
    mountpoint: PathBuf,
    overlay: Option<PathBuf>,
    daemon: bool,
    rw: bool,
    cache_size: Option<String>,
    uid: u32,
    gid: u32,
    nbd: bool,
) -> Result<()> {
    if nbd {
        #[cfg(feature = "server")]
        return run_nbd(strata_path, mountpoint, cache_size);
        #[cfg(not(feature = "server"))]
        anyhow::bail!("NBD support requires the 'server' feature");
    }

    // --- FUSE Implementation ---

    let abs_mountpoint = std::fs::canonicalize(&mountpoint)
        .context(format!("Failed to resolve mountpoint: {:?}", mountpoint))?;

    // FIX: Don't use canonicalize on the overlay path directly, as it fails if the file doesn't exist.
    // Instead, resolve it relative to current dir if needed, or just pass it through.
    // Strata::new handles creation.
    let abs_overlay_path = if let Some(p) = &overlay {
        if p.is_absolute() {
            Some(p.clone())
        } else {
            Some(std::env::current_dir()?.join(p))
        }
    } else {
        None
    };

    // Open snapshot
    let snap = open_snapshot(&strata_path, cache_size)?;

    // Daemonize if requested
    if daemon {
        let stdout = std::fs::File::create("/tmp/strata.log")
            .unwrap_or_else(|_| std::fs::File::create("/dev/null").unwrap());
        let stderr = std::fs::File::create("/tmp/strata.err")
            .unwrap_or_else(|_| std::fs::File::create("/dev/null").unwrap());

        Daemonize::new()
            .working_directory("/")
            .stdout(stdout)
            .stderr(stderr)
            .start()?;
    }

    // Handle RW overlay
    let (_temp_file, final_overlay_path) = if let Some(p) = abs_overlay_path {
        (None, Some(p))
    } else if rw {
        let t = tempfile::NamedTempFile::new()?;
        let path = t.path().to_path_buf();
        let meta = path.with_extension("meta");
        std::fs::File::create(&meta)?;
        (Some(t), Some(path))
    } else {
        (None, None)
    };

    let mut options = vec![
        fuser::MountOption::FSName("strata".to_string()),
        fuser::MountOption::DefaultPermissions,
    ];

    if rw {
        options.push(fuser::MountOption::RW);
    } else {
        options.push(fuser::MountOption::RO);
    }

    let fs = strata_fuse::fuse::Strata::new(snap, final_overlay_path.as_deref(), uid, gid)?;

    if daemon {
        eprintln!("Mounting at {:?} (daemonized)", abs_mountpoint);
    }

    fuser::mount2(fs, abs_mountpoint, &options)?;

    Ok(())
}

#[cfg(feature = "server")]
fn run_nbd(strata_path: String, mountpoint: PathBuf, cache_size: Option<String>) -> Result<()> {
    // 1. Check for sudo/root (NBD requires it)
    let is_root = unsafe { libc::geteuid() == 0 };
    if !is_root {
        println!("Note: NBD mounting requires root privileges. You may be prompted for sudo.");
    }

    // 2. Open Snapshot
    let snap = open_snapshot(&strata_path, cache_size)?;

    // 3. Find a free NBD device
    let nbd_dev = find_free_nbd_device()?;
    println!("Selected NBD device: {}", nbd_dev);

    // 4. Start Server in background runtime
    let rt = tokio::runtime::Runtime::new()?;

    // Bind to ephemeral port
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener); // Close so server can bind

    println!("Starting internal NBD server on port {}...", port);

    let snap_clone = snap.clone();
    rt.spawn(async move {
        if let Err(e) = strata_server::serve_nbd(snap_clone, port).await {
            eprintln!("NBD Server error: {}", e);
        }
    });

    // Give server a moment to start
    std::thread::sleep(std::time::Duration::from_millis(200));

    // 5. Connect Client
    println!("Connecting NBD client...");
    let status = Command::new("sudo")
        .arg("nbd-client")
        .arg("localhost")
        .arg(port.to_string())
        .arg(&nbd_dev)
        .status()
        .context("Failed to run nbd-client")?;

    if !status.success() {
        anyhow::bail!("nbd-client failed. Is the 'nbd' kernel module loaded?");
    }

    // 6. Mount
    println!("Mounting {} to {:?}...", nbd_dev, mountpoint);
    let status = Command::new("sudo")
        .arg("mount")
        .arg(&nbd_dev)
        .arg(&mountpoint)
        .status()
        .context("Failed to run mount")?;

    if !status.success() {
        // Cleanup NBD if mount fails
        let _ = Command::new("sudo")
            .arg("nbd-client")
            .arg("-d")
            .arg(&nbd_dev)
            .status();
        anyhow::bail!("Mount command failed.");
    }

    println!("Successfully mounted via NBD.");
    println!("Press Ctrl+C to unmount and cleanup.");

    // 7. Wait for Ctrl+C
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    while running.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // 8. Cleanup
    println!("\nCleaning up...");

    let _ = Command::new("sudo").arg("umount").arg(&mountpoint).status();
    let _ = Command::new("sudo")
        .arg("nbd-client")
        .arg("-d")
        .arg(&nbd_dev)
        .status();

    Ok(())
}

#[cfg(feature = "server")]
fn find_free_nbd_device() -> Result<String> {
    // Simple heuristic: Try to find a device that isn't connected.
    // We check /sys/class/block/nbd*/pid. If file exists and contains content, it's busy.
    // Or simpler: /sys/class/block/nbd*/size is 0 for disconnected devices.

    for i in 0..16 {
        let dev = format!("/dev/nbd{}", i);
        let sys_path = format!("/sys/class/block/nbd{}/size", i);

        if Path::new(&sys_path).exists() {
            let content = std::fs::read_to_string(&sys_path).unwrap_or_default();
            if content.trim() == "0" {
                return Ok(dev);
            }
        }
    }
    anyhow::bail!("No free /dev/nbd devices found. Try 'sudo modprobe nbd max_part=8'")
}

#[cfg(test)]
mod tests {
    use super::parse_size;

    #[test]
    fn test_parse_size_megabytes() {
        assert_eq!(parse_size("256M").unwrap(), 268435456);
    }

    #[test]
    fn test_parse_size_fractional_gigabytes() {
        assert_eq!(parse_size("1.5G").unwrap(), 1610612736);
    }

    #[test]
    fn test_parse_size_kilobytes_suffix() {
        assert_eq!(parse_size("512KB").unwrap(), 524288);
    }

    #[test]
    fn test_parse_size_plain_number() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
    }

    #[test]
    fn test_parse_size_k_suffix() {
        assert_eq!(parse_size("1K").unwrap(), 1024);
    }

    #[test]
    fn test_parse_size_terabytes() {
        assert_eq!(parse_size("1T").unwrap(), 1099511627776);
    }

    #[test]
    fn test_parse_size_zero() {
        assert_eq!(parse_size("0").unwrap(), 0);
    }

    #[test]
    fn test_parse_size_invalid_suffix() {
        assert!(parse_size("100X").is_err());
    }

    #[test]
    fn test_parse_size_non_numeric() {
        assert!(parse_size("abc").is_err());
    }

    #[test]
    fn test_parse_size_mb_suffix() {
        assert_eq!(parse_size("1MB").unwrap(), 1048576);
    }

    #[test]
    fn test_parse_size_gb_suffix() {
        assert_eq!(parse_size("2GB").unwrap(), 2147483648);
    }

    #[test]
    fn test_parse_size_whitespace() {
        assert_eq!(parse_size("  256M  ").unwrap(), 268435456);
    }
}
