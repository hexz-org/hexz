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

fn parse_size(s: &str) -> Result<usize> {
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

// Helper to open the snapshot (shared by FUSE and NBD paths)
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
