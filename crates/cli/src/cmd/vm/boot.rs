//! Boot a VM from a Hexz snapshot with optional persistence.
//!
//! This command implements the full VM boot workflow, combining FUSE mounting,
//! overlay management, and hypervisor configuration to launch virtual machines
//! directly from compressed Hexz snapshots.
//!
//! # Boot Process
//!
//! 1. **Mount Snapshot**: Mount the `.st` archive via FUSE as a block device
//! 2. **Create Overlay**: Set up writable overlay if `--persist` specified
//! 3. **Launch Hypervisor**: Start QEMU with configured parameters
//! 4. **Setup QMP**: Connect to QEMU Machine Protocol for control
//! 5. **Resume VM**: Trigger execution if VM is paused
//!
//! # Hypervisor Support
//!
//! - **QEMU** (default): Full featured, supports KVM acceleration
//! - **Firecracker**: Lightweight microVM (future support)
//!
//! # Persistence Modes
//!
//! **Ephemeral** (no overlay):
//! ```bash
//! hexz vm boot snapshot.st
//! # Changes lost on shutdown
//! ```
//!
//! **Persistent** (with overlay):
//! ```bash
//! hexz vm boot snapshot.st --persist overlay.bin
//! # Writes saved to overlay.bin
//! # Commit with: hexz vm commit snapshot.st overlay.bin new-snapshot.st
//! ```
//!
//! # Performance Features
//!
//! - **KVM Acceleration**: Enabled by default (disable with `--no-kvm`)
//! - **Transparent Decompression**: FUSE layer handles LZ4/Zstd on-the-fly
//! - **Block Cache**: Reduces repeated decompression overhead

use anyhow::{Context, Result};
use hexz_common::constants::DEFAULT_ZSTD_LEVEL;
use hexz_core::format::magic::HEADER_SIZE;
use hexz_core::store::StorageBackend;
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// Maximum retries while waiting for the FUSE mount to expose the disk file (50).
///
/// **Architectural intent:** Allows up to ~5 seconds (50 × 100 ms) for the
/// mount thread to initialize and expose the root directory; exceeding this
/// aborts boot with a timeout error.
const MOUNT_WAIT_RETRIES: usize = 50;

/// Sleep duration between mount readiness checks (100 ms).
///
/// **Architectural intent:** Reduces CPU spin while waiting for the mount;
/// shorter intervals increase responsiveness at the cost of more wakeups.
const MOUNT_WAIT_SLEEP: Duration = Duration::from_millis(100);

/// Maximum time to wait for QEMU to become ready for QMP after launch (10 s).
///
/// **Architectural intent:** Covers VM init and incoming migration setup;
/// exceeding this may leave the VM in a paused state if QMP connect fails.
const QEMU_INIT_TIMEOUT: Duration = Duration::from_secs(10);

/// Number of QMP status polls after connecting before giving up (20).
///
/// **Architectural intent:** Allows the VM to transition from paused/postmigrate
/// to running; each iteration sleeps `QMP_POLL_SLEEP` so total wait is bounded.
const QMP_POLL_ITERATIONS: usize = 20;

/// Interval between QMP status polls while waiting for VM state (500 ms).
///
/// **Architectural intent:** Balances responsiveness with QMP socket load;
/// too short increases traffic, too long delays resume detection.
const QMP_POLL_SLEEP: Duration = Duration::from_millis(500);

/// Sleep between QMP socket connect attempts (200 ms).
///
/// **Architectural intent:** Gives QEMU time to create the socket; shorter
/// intervals may spin before the socket exists.
const QMP_CONNECT_RETRY_SLEEP: Duration = Duration::from_millis(200);

/// Boots a virtual machine from a Hexz snapshot with optional persistence.
///
/// **Architectural intent:** Mounts the snapshot via FUSE, configures an
/// overlay for writable state, and then launches QEMU with appropriate
/// parameters (RAM, KVM, networking, QMP) so the guest can run directly from
/// the snapshot image.
///
/// **Constraints:** Requires `qemu-system-x86_64` and FUSE support on the
/// host. When `persist_path` is omitted, an ephemeral overlay is used and
/// discarded on exit; callers must supply `persist_path` if they want changes
/// to survive reboots.
///
/// **Side effects:** Creates temporary directories and overlay files, spawns a
/// FUSE mount thread and a QEMU process, manipulates networking, and may
/// leave resources behind if the process is terminated abruptly.
#[allow(clippy::too_many_arguments)]
pub fn run(
    snap_path: String,
    ram_size: Option<String>,
    kernel_mode: bool,
    persist_path: Option<PathBuf>,
    qmp_socket: Option<PathBuf>,
    network_mode: String,
    backend_type: String,
    no_graphics: bool,
    vnc: bool,
) -> Result<()> {
    match backend_type.as_str() {
        "qemu" => boot_qemu(
            snap_path,
            ram_size,
            kernel_mode,
            persist_path,
            qmp_socket,
            network_mode,
            no_graphics,
            vnc,
        ),
        "firecracker" => {
            #[cfg(feature = "firecracker")]
            return boot_firecracker(snap_path, ram_size, persist_path, network_mode);
            #[cfg(not(feature = "firecracker"))]
            anyhow::bail!(
                "Firecracker backend is not available. Compile with --features firecracker"
            )
        }
        other => anyhow::bail!("Unknown backend: {}", other),
    }
}

#[cfg(feature = "firecracker")]
fn boot_firecracker(
    _snap_path: String,
    _ram_size: Option<String>,
    _persist_path: Option<PathBuf>,
    _network_mode: String,
) -> anyhow::Result<()> {
    // TODO: Implement Firecracker boot orchestration.
    // 1. Generate a JSON configuration for Firecracker.
    // 2. Map the FUSE mount point (or a tap device) as the root drive.
    // 3. Spawn the `firecracker` process and send the config via its API socket.
    // 4. Handle console output and cleanup.
    anyhow::bail!("Firecracker backend is not yet fully implemented.")
}

fn boot_qemu(
    snap_path: String,
    ram_size: Option<String>,
    kernel_mode: bool,
    persist_path: Option<PathBuf>,
    qmp_socket: Option<PathBuf>,
    network_mode: String,
    no_graphics: bool,
    vnc: bool,
) -> Result<()> {
    let mount_dir = tempfile::tempdir().context("Failed to create temp mount dir")?;
    let mount_path = mount_dir.path().to_path_buf();

    let (overlay_path, _temp_guard) = if let Some(p) = persist_path {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }

        let meta = p.with_extension("meta");
        if !meta.exists() {
            std::fs::File::create(&meta)?;
        }

        if !p.exists() {
            std::fs::File::create(&p).context("Failed to create persistent overlay file")?;
        }
        (p, None)
    } else {
        let t = tempfile::NamedTempFile::new()?;
        let p = t.path().to_path_buf();

        let meta = p.with_extension("meta");
        std::fs::File::create(&meta)?;

        (p, Some(t))
    };

    println!("Mounting {} at {}...", snap_path, mount_path.display());

    println!("Overlay path: {:?}", overlay_path);
    if _temp_guard.is_some() {
        println!("(Ephemeral mode: Overlay will be deleted on exit)");
    }

    let snap_path_clone = snap_path.clone();
    let mount_path_clone = mount_path.clone();
    let overlay_path_clone = overlay_path.clone();

    let mounted_flag = Arc::new(AtomicBool::new(false));
    let mounted_clone = mounted_flag.clone();

    let mount_handle = thread::spawn(move || -> Result<()> {
        let backend = Arc::new(
            hexz_core::store::local::file::FileBackend::new(std::path::Path::new(&snap_path_clone))
                .context("Failed to open snapshot file")?,
        );

        let header_bytes = backend
            .read_exact(0, HEADER_SIZE)
            .context("Failed to read header")?;
        let header: hexz_core::format::header::Header =
            bincode::deserialize(&header_bytes).context("Failed to deserialize header")?;

        let compressor: Box<dyn hexz_core::algo::compression::Compressor> = match header.compression
        {
            hexz_core::format::header::CompressionType::Zstd => {
                let dict = if let (Some(off), Some(len)) =
                    (header.dictionary_offset, header.dictionary_length)
                {
                    Some(
                        backend
                            .read_exact(off, len as usize)
                            .context("Failed to read dictionary")?
                            .to_vec(),
                    )
                } else {
                    None
                };
                Box::new(hexz_core::algo::compression::zstd::ZstdCompressor::new(
                    DEFAULT_ZSTD_LEVEL,
                    dict,
                ))
            }
            _ => Box::new(hexz_core::algo::compression::lz4::Lz4Compressor::new()),
        };

        let snap =
            hexz_core::File::new(backend, compressor, None).context("Failed to create File")?;

        mounted_clone.store(true, Ordering::Release);

        // Default to UID/GID 1000 for boot VM mount
        hexz_fuse::mount_fs(
            snap,
            &mount_path_clone,
            Some(&overlay_path_clone),
            1000,
            1000,
        )
        .context("Mount failed")?;

        Ok(())
    });

    print!("Waiting for mount...");
    let mut retries = 0;
    loop {
        if retries > MOUNT_WAIT_RETRIES {
            anyhow::bail!("Timed out waiting for mount");
        }
        if mount_handle.is_finished() {
            match mount_handle.join() {
                Ok(Err(e)) => return Err(e.context("Mount thread failed")),
                Ok(Ok(())) => anyhow::bail!("Mount thread exited unexpectedly (unmounted?)"),
                Err(e) => std::panic::resume_unwind(e),
            }
        }
        if mounted_flag.load(Ordering::Acquire) && mount_path.join("disk").exists() {
            println!(" Ready.");
            break;
        }
        thread::sleep(MOUNT_WAIT_SLEEP);
        print!(".");
        use std::io::Write;
        std::io::stdout().flush()?;
        retries += 1;
    }

    let disk_path = mount_path.join("disk");
    let mem_path = mount_path.join("memory");
    let has_memory = mem_path.exists();

    let memory_arg = if let Some(r) = ram_size {
        r
    } else {
        if has_memory {
            println!("! Warning: RAM size not specified. Defaulting to 4G.");
        }
        "4G".to_string()
    };

    println!("Booting VM (RAM: {})...", memory_arg);

    let internal_qmp = tempfile::NamedTempFile::new()?;
    let internal_qmp_path = internal_qmp.path().to_path_buf();
    let _ = std::fs::remove_file(&internal_qmp_path);

    let mut qemu = Command::new("qemu-system-x86_64");
    qemu.arg("-m").arg(&memory_arg);

    if vnc {
        println!("Starting VNC server on display :1 (Port 5901).");
        qemu.arg("-display").arg("vnc=:1");
    } else if no_graphics {
        println!("(Running in Headless Serial Mode)");
        println!("* To exit QEMU: Press 'Ctrl+a' then 'x'");

        qemu.arg("-nographic");
    }

    if network_mode == "user" {
        println!("Networking enabled (user/virtio)");
        qemu.arg("-net").arg("nic,model=virtio");
        qemu.arg("-net").arg("user");
    } else if network_mode == "tap" {
        println!("Networking enabled (tap)");
        qemu.arg("-net").arg("nic,model=virtio");
        qemu.arg("-net").arg("tap");
    } else {
        println!("Networking disabled (strict isolation)");
        qemu.arg("-net").arg("none");
    }

    qemu.arg("-drive")
        .arg(format!("file={},format=raw", disk_path.display()));

    if kernel_mode {
        qemu.arg("-enable-kvm");
    }

    qemu.arg("-qmp").arg(format!(
        "unix:{},server,nowait",
        internal_qmp_path.to_string_lossy()
    ));

    if let Some(socket) = qmp_socket {
        if socket.exists() {
            let _ = std::fs::remove_file(&socket);
        }
        println!("QMP Socket enabled at: {:?}", socket);
        qemu.arg("-qmp")
            .arg(format!("unix:{},server,nowait", socket.to_string_lossy()));
    }

    if has_memory {
        let mem_path_str = mem_path.to_string_lossy().replace('\'', "'\\''");
        qemu.arg("-incoming")
            .arg(format!("exec:cat '{}'", mem_path_str));
    }

    let mut child = qemu.spawn().context("Failed to run qemu-system-x86_64")?;

    if has_memory {
        println!("Waiting for VM to initialize...");

        let start_time = Instant::now();
        let timeout = QEMU_INIT_TIMEOUT;
        let mut connected = false;

        while start_time.elapsed() < timeout {
            if let Ok(Some(status)) = child.try_wait() {
                anyhow::bail!("QEMU process exited unexpectedly with status: {}", status);
            }

            if let Ok(mut stream) = UnixStream::connect(&internal_qmp_path) {
                connected = true;

                let _ = read_qmp_response(&mut stream);
                let _ = send_qmp_command(&mut stream, "qmp_capabilities");

                println!("Connected to QEMU. Polling status...");
                for _ in 0..QMP_POLL_ITERATIONS {
                    if let Ok(resp) = send_qmp_command(&mut stream, "query-status")
                        && let Some(ret) = resp.get("return")
                        && let Some(status) = ret.get("status").and_then(|s| s.as_str())
                    {
                        if status == "paused" || status == "postmigrate" || status == "prelaunch" {
                            println!("VM State: {}. Sending resume command...", status);
                            let _ = send_qmp_command(&mut stream, "cont");
                            break;
                        } else if status == "running" {
                            println!("VM is running.");
                            break;
                        }
                    }
                    thread::sleep(QMP_POLL_SLEEP);
                }
                break;
            }
            thread::sleep(QMP_CONNECT_RETRY_SLEEP);
        }

        if !connected {
            eprintln!("! Warning: Failed to connect to QEMU QMP socket. VM may be frozen.");
        }
    }

    let status = child.wait()?;

    if !status.success() {
        eprintln!("QEMU exited with error");
    }

    println!("Cleaning up...");
    let _ = Command::new("fusermount")
        .arg("-u")
        .arg(&mount_path)
        .status();

    match mount_handle.join() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!("Mount thread returned error: {}", e),
        Err(e) => eprintln!("Mount thread panicked: {:?}", e),
    }

    if _temp_guard.is_some() {
        let meta_path = overlay_path.with_extension("meta");
        if meta_path.exists() {
            let _ = std::fs::remove_file(meta_path);
        }
    }

    Ok(())
}

/// Sends a QMP command and returns the parsed response.
///
/// **Architectural intent:** Wraps the low-level JSON encoding and streaming
/// details required to talk to QEMU's QMP control socket, so callers can work
/// with simple Rust types and command strings.
///
/// **Constraints:** The `cmd` must be a valid QMP command; error responses are
/// returned as JSON values and are not interpreted here.
///
/// **Side effects:** Performs blocking writes and reads on the QMP Unix
/// socket and allocates temporary buffers and JSON structures for each call.
fn send_qmp_command(stream: &mut UnixStream, cmd: &str) -> Result<Value> {
    let json = serde_json::json!({ "execute": cmd });
    let data = serde_json::to_string(&json)?;
    stream.write_all(data.as_bytes())?;
    read_qmp_response(stream)
}

/// Reads a single QMP response and returns the first object with a `return` field.
///
/// **Architectural intent:** Normalizes QMP's stream of JSON messages into a
/// single value corresponding to the most recent command, ignoring
/// out-of-band events.
///
/// **Constraints:** Assumes individual responses fit within a fixed 4 KiB
/// buffer and are line-delimited JSON; unusually large or fragmented
/// responses may require revisiting this implementation.
///
/// **Side effects:** Performs a blocking read from the QMP socket and parses
/// JSON, allocating transient strings and value structures.
fn read_qmp_response(stream: &mut UnixStream) -> Result<Value> {
    let mut buf = [0u8; HEADER_SIZE];
    let n = stream.read(&mut buf)?;
    let s = String::from_utf8_lossy(&buf[..n]);

    for line in s.lines() {
        if let Ok(val) = serde_json::from_str::<Value>(line)
            && val.get("return").is_some()
        {
            return Ok(val);
        }
    }
    Ok(serde_json::json!({}))
}
