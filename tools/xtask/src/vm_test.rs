use crate::common::*;
use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Child;

const KERNEL_URL: &str =
    "https://dl-cdn.alpinelinux.org/alpine/v3.19/releases/x86_64/netboot/vmlinuz-virt";
const INITRAMFS_URL: &str =
    "https://dl-cdn.alpinelinux.org/alpine/v3.19/releases/x86_64/netboot/initramfs-virt";

const BENCH_INIT_SCRIPT: &str = r#"#!/bin/busybox sh
/bin/busybox --install /bin
mount -t devtmpfs devtmpfs /dev
mount -t proc proc /proc
mount -t sysfs sysfs /sys

echo "--- Bench Start ---"
modprobe virtio
modprobe virtio_pci
modprobe virtio_blk
sleep 0.5
mdev -s

if [ -b /dev/vda ]; then
    echo "Reading /dev/vda..."
    time dd if=/dev/vda of=/dev/null bs=1M status=noxfer
else
    echo "Error: /dev/vda not found"
fi
echo "--- Bench End ---"
sync
poweroff -f
"#;

/// Guard struct for RAII cleanup of FUSE mount.
struct VmTestGuard {
    bin: PathBuf,
    mount_dir: PathBuf,
    fuse_child: Option<Child>,
}

impl Drop for VmTestGuard {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.fuse_child {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = cmd(self.bin.to_str().unwrap_or("hexz"))
            .args(["vm", "unmount", &self.mount_dir.to_string_lossy()])
            .run_with_status();
        let _ = cmd("fusermount")
            .args(["-u", &self.mount_dir.to_string_lossy()])
            .run_with_status();
    }
}

pub fn run() -> Result<()> {
    let root = find_workspace_root()?;
    let bin = root.join("target/release/hexz");
    let bin_str = bin.to_str().unwrap();

    require_cmd("qemu-system-x86_64")?;
    require_cmd("cpio")?;
    require_cmd("gzip")?;

    println!("{CYAN}=== Hexz VM Test ==={RESET}");
    println!("{CYAN}BIN={bin_str}{RESET}");

    // Build if needed
    if !bin.exists() {
        println!("{CYAN}Building hexz (release)\u{2026}{RESET}");
        cmd(cargo())
            .args(["build", "--release", "--workspace"])
            .current_dir(&root)
            .run()?;
    }

    let work_dir = root.join("vm_test_work");
    let mount_dir = work_dir.join("mnt");
    std::fs::create_dir_all(&mount_dir)?;

    // Download kernel
    let vmlinuz = work_dir.join("vmlinuz");
    if !vmlinuz.exists() {
        println!("{CYAN}Downloading Linux Kernel...{RESET}");
        download_file(KERNEL_URL, &vmlinuz)?;
    }

    // Download initramfs
    let initramfs = work_dir.join("initramfs");
    if !initramfs.exists() {
        println!("{CYAN}Downloading Initramfs...{RESET}");
        download_file(INITRAMFS_URL, &initramfs)?;
    }

    // Create compressible disk image
    let image_raw = work_dir.join("disk.raw");
    if !image_raw.exists() {
        println!("{CYAN}Creating 200MB compressible raw disk image...{RESET}");
        create_compressible_image(&image_raw, 200)?;
    }

    // Create benchmark initramfs
    let initramfs_bench = work_dir.join("initramfs-bench");
    if !initramfs_bench.exists() {
        println!("{CYAN}Building Custom Benchmark Initramfs...{RESET}");
        build_bench_initramfs(&initramfs, &initramfs_bench, &work_dir)?;
    }

    // Create snapshot
    let image_snap = work_dir.join("disk.hxz");
    println!("{CYAN}Converting Raw Image to Hexz Snapshot...{RESET}");
    if image_snap.exists() {
        std::fs::remove_file(&image_snap)?;
    }
    cmd(bin_str)
        .args([
            "data",
            "pack",
            "--disk",
            image_raw.to_str().unwrap(),
            "--output",
            image_snap.to_str().unwrap(),
        ])
        .run()?;

    // Mount via FUSE
    println!("{CYAN}Mounting Snapshot...{RESET}");
    let _ = cmd("fusermount")
        .args(["-u", mount_dir.to_str().unwrap()])
        .run_with_status();

    let fuse_log = work_dir.join("fuse.log");
    let fuse_log_file = std::fs::File::create(&fuse_log)?;
    let fuse_child = std::process::Command::new(bin_str)
        .args([
            "vm",
            "mount",
            image_snap.to_str().unwrap(),
            mount_dir.to_str().unwrap(),
        ])
        .stdout(fuse_log_file.try_clone()?)
        .stderr(fuse_log_file)
        .spawn()?;

    let guard = VmTestGuard {
        bin: bin.clone(),
        mount_dir: mount_dir.clone(),
        fuse_child: Some(fuse_child),
    };

    std::thread::sleep(std::time::Duration::from_secs(1));

    // Verify mount
    let disk_file = mount_dir.join("disk");
    if !disk_file.exists() {
        anyhow::bail!("Mount failed! Check {}", fuse_log.display());
    }

    println!("{CYAN}Mount successful. Verifying read access...{RESET}");
    cmd("dd")
        .args([
            &format!("if={}", disk_file.display()),
            "of=/dev/null",
            "bs=1M",
            "count=1",
            "status=none",
        ])
        .run()
        .context("Failed to read from mount")?;
    println!("{GREEN}Successfully read from mounted snapshot!{RESET}");

    // Boot QEMU
    println!("{CYAN}Booting QEMU with mounted snapshot...{RESET}");
    let boot_start = std::time::Instant::now();

    cmd("qemu-system-x86_64")
        .args([
            "-kernel",
            vmlinuz.to_str().unwrap(),
            "-initrd",
            initramfs_bench.to_str().unwrap(),
            "-append",
            "console=ttyS0 quiet",
            "-drive",
            &format!("file={},format=raw,if=virtio", disk_file.display()),
            "-snapshot",
            "-nographic",
            "-m",
            "512",
            "-no-reboot",
        ])
        .run()?;

    let boot_duration = boot_start.elapsed();
    println!(
        "{GREEN}QEMU exited. Boot duration: {:.2}s{RESET}",
        boot_duration.as_secs_f64()
    );

    // Explicit cleanup
    drop(guard);

    println!("{GREEN}Test Complete.{RESET}");
    Ok(())
}

fn download_file(url: &str, dest: &Path) -> Result<()> {
    let resp = ureq::get(url)
        .set("User-Agent", "hexz-xtask")
        .call()
        .with_context(|| format!("downloading {url}"))?;

    let mut file = std::fs::File::create(dest)?;
    let mut reader = resp.into_reader();
    std::io::copy(&mut reader, &mut file)?;
    Ok(())
}

fn create_compressible_image(path: &Path, size_mb: usize) -> Result<()> {
    let pattern = b"This is a test of Hexz compression speed. We want some repeated text that compresses well.\n";
    let mut file = std::fs::File::create(path)?;
    let total_bytes = size_mb * 1024 * 1024;
    let mut written = 0;
    while written < total_bytes {
        let to_write = std::cmp::min(pattern.len(), total_bytes - written);
        file.write_all(&pattern[..to_write])?;
        written += to_write;
    }
    Ok(())
}

fn build_bench_initramfs(original_initramfs: &Path, output: &Path, work_dir: &Path) -> Result<()> {
    let build_dir = work_dir.join("initramfs_build");
    std::fs::create_dir_all(&build_dir)?;

    // Extract original initramfs
    let _ = cmd("sh")
        .args([
            "-c",
            &format!(
                "cd {} && zcat {} | cpio -idm 2>/dev/null || true",
                build_dir.display(),
                original_initramfs.display()
            ),
        ])
        .run_with_status();

    // Write custom init script
    let init_path = build_dir.join("init");
    std::fs::write(&init_path, BENCH_INIT_SCRIPT)?;

    // chmod +x init
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&init_path, std::fs::Permissions::from_mode(0o755))?;
    }

    // Build new initramfs
    cmd("sh")
        .args([
            "-c",
            &format!(
                "cd {} && find . | cpio -o -H newc 2>/dev/null | gzip > {}",
                build_dir.display(),
                output.display()
            ),
        ])
        .run()?;

    std::fs::remove_dir_all(&build_dir)?;
    Ok(())
}
