//! Mount Hexz archives as FUSE filesystems.

use anyhow::{Context, Result};
use daemonize::Daemonize;
use hexz_common::constants::DEFAULT_ZSTD_LEVEL;
use hexz_core::Archive;
use hexz_core::algo::compression::{Compressor, lz4::Lz4Compressor, zstd::ZstdCompressor};
use hexz_core::algo::encryption::aes_gcm::AesGcmEncryptor;
use hexz_core::format::header::{CompressionType, Header};
use hexz_core::format::magic::HEADER_SIZE;
use hexz_fuse::fuse::Hexz;
use hexz_store::StorageBackend;
use hexz_store::local::MmapBackend;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use colored::Colorize;

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
        _ => return Err(anyhow::anyhow!("Invalid size suffix: {suffix}")),
    };

    Ok((n * multiplier) as usize)
}

pub(crate) fn open_archive(
    hexz_path: &str,
    cache_size: Option<&str>,
    prefetch: Option<u32>,
) -> Result<Arc<Archive>> {
    let abs_hexz_path = std::fs::canonicalize(hexz_path)
        .context(format!("Failed to resolve archive path: {hexz_path}"))?;

    let (header, password) = {
        let backend = MmapBackend::new(&abs_hexz_path)?;
        let header_bytes = backend.read_exact(0, HEADER_SIZE)?;
        let header: Header = bincode::deserialize(&header_bytes)?;

        let password = if header.encryption.is_some() {
            Some(rpassword::prompt_password("Enter encryption password: ")?)
        } else {
            None
        };
        (header, password)
    };

    let backend = Arc::new(MmapBackend::new(&abs_hexz_path)?);

    let dictionary = if let (Some(offset), Some(length)) =
        (header.dictionary_offset, header.dictionary_length)
    {
        Some(backend.read_exact(offset, length as usize)?.to_vec())
    } else {
        None
    };

    let compressor: Box<dyn Compressor> = match header.compression {
        CompressionType::Lz4 => Box::new(Lz4Compressor::new()),
        CompressionType::Zstd => Box::new(ZstdCompressor::new(DEFAULT_ZSTD_LEVEL, dictionary.as_deref())),
    };

    let encryptor = if let (Some(params), Some(pass)) = (header.encryption, password) {
        Some(Box::new(AesGcmEncryptor::new(
            pass.as_bytes(),
            &params.salt,
            params.iterations,
        )?)
            as Box<dyn hexz_core::algo::encryption::Encryptor>)
    } else {
        None
    };

    let cache_capacity = if let Some(s) = cache_size {
        Some(parse_size(s)?)
    } else {
        None
    };

    let cache_size_owned: Option<String> = cache_size.map(String::from);
    let abs_hexz_path_clone = abs_hexz_path;

    let parent_loader: hexz_core::api::file::ParentLoader = Box::new(move |parent_path: &str| {
        let parent_full_path = abs_hexz_path_clone
            .parent()
            .ok_or_else(|| hexz_common::Error::Io(std::io::Error::other("archive path has no parent directory")))?
            .join(parent_path);
        let path_str = parent_full_path
            .to_str()
            .ok_or_else(|| hexz_common::Error::Io(std::io::Error::other("parent path is not valid UTF-8")))?;
        open_archive(path_str, cache_size_owned.as_deref(), prefetch)
            .map_err(|e| hexz_common::Error::Io(std::io::Error::other(e.to_string())))
    });

    Ok(Archive::with_cache_and_loader(
        backend,
        compressor,
        encryptor,
        cache_capacity,
        prefetch,
        Some(&parent_loader),
    )?)
}

/// Execute the `hexz mount` command to mount an archive as a FUSE filesystem.
#[allow(clippy::too_many_arguments, unsafe_code)]
pub fn run(
    hexz_path: &str,
    mountpoint: &Path,
    daemon: bool,
    cache_size: Option<&str>,
    mut uid: u32,
    mut gid: u32,
    overlay: Option<PathBuf>,
    editable: bool,
    metadata_dir: Option<&Path>,
) -> Result<()> {
    if uid == 0 {
        // SAFETY: getuid() is always safe to call
        uid = unsafe { libc::getuid() };
    }
    if gid == 0 {
        // SAFETY: getgid() is always safe to call
        gid = unsafe { libc::getgid() };
    }

    let abs_mountpoint = if mountpoint.exists() {
        std::fs::canonicalize(mountpoint)
            .context(format!("Failed to resolve mountpoint: {}", mountpoint.display()))?
    } else {
        mountpoint.to_path_buf()
    };

    let snap = open_archive(hexz_path, cache_size, None)?;

    // Handle --editable / --overlay
    let overlay = if let Some(o) = overlay {
        std::fs::create_dir_all(&o)?;
        Some(o)
    } else if editable {
        let temp_overlay = std::env::temp_dir().join(format!(
            "hexz_overlay_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        ));
        std::fs::create_dir_all(&temp_overlay)?;
        if !daemon {
            println!("  {} Editable mode enabled. Overlay: {}", "→".yellow(), temp_overlay.display().to_string().bright_black());
        }
        Some(temp_overlay)
    } else {
        None
    };

    if daemon {
        let log_dir = std::env::var("XDG_RUNTIME_DIR")
            .or_else(|_| std::env::var("TMPDIR"))
            .unwrap_or_else(|_| "/tmp".to_string());
        let stdout = std::fs::File::create(format!("{log_dir}/hexz.log"))
            .or_else(|_| std::fs::File::create("/dev/null"))
            .context("Failed to create log file")?;
        let stderr = std::fs::File::create(format!("{log_dir}/hexz.err"))
            .or_else(|_| std::fs::File::create("/dev/null"))
            .context("Failed to create error log file")?;

        Daemonize::new()
            .working_directory("/")
            .stdout(stdout)
            .stderr(stderr)
            .start()?;
    }

    let mut options = vec![
        fuser::MountOption::FSName("hexz".to_string()),
        fuser::MountOption::DefaultPermissions,
    ];

    if overlay.is_none() {
        options.push(fuser::MountOption::RO);
    }

    let fs = Hexz::new(snap, uid, gid, overlay, metadata_dir)?;

    if daemon {
        eprintln!("  {} Mounting at {} (daemonized)", "✓".green(), abs_mountpoint.display().to_string().cyan());
    }

    fuser::mount2(fs, abs_mountpoint, &options)?;

    Ok(())
}
