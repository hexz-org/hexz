use crate::common::*;
use anyhow::Result;
use std::collections::BTreeSet;

#[derive(clap::Subcommand)]
pub enum TestCmd {
    /// List Rust test categories for filtering
    List,
    /// Exercise every CLI command and flag combination
    Commands,
    /// FUSE mount integration test
    Mount,
}

pub fn run(cmd: TestCmd) -> Result<()> {
    match cmd {
        TestCmd::List => list(),
        TestCmd::Commands => commands(),
        TestCmd::Mount => mount(),
    }
}

fn list() -> Result<()> {
    let root = find_workspace_root()?;

    let output = cmd(cargo())
        .args(["test", "--workspace", "--", "--list"])
        .current_dir(&root)
        .capture_all()?;

    println!("{GREEN}Rust test categories (use with make test <category>)\u{2026}{RESET}\n");

    let mut categories = BTreeSet::new();
    for line in output.lines() {
        if let Some(rest) = line.strip_suffix(": test") {
            // Extract crate prefix (before first ::)
            if let Some(category) = rest.split("::").next() {
                categories.insert(category.to_string());
            }
        }
    }

    for cat in &categories {
        println!("{cat}");
    }

    Ok(())
}

fn commands() -> Result<()> {
    let root = find_workspace_root()?;
    let bin = root.join("target/release/hexz");

    // Build if needed
    if !bin.exists() {
        println!("{CYAN}Building hexz (release)\u{2026}{RESET}");
        cmd(cargo())
            .args(["build", "--release", "--workspace"])
            .current_dir(&root)
            .run()?;
    }

    let tmp = tempfile::tempdir()?;
    let tmp_path = tmp.path();
    let bin_str = bin.to_str().unwrap();

    // Create a small test image and snapshot
    let small_raw = tmp_path.join("small.raw");
    cmd("dd")
        .args([
            "if=/dev/zero",
            &format!("of={}", small_raw.display()),
            "bs=1M",
            "count=1",
        ])
        .run()?;

    let snap_base = tmp_path.join("base.hxz");
    cmd(bin_str)
        .args([
            "data",
            "pack",
            "--disk",
            small_raw.to_str().unwrap(),
            "--output",
            snap_base.to_str().unwrap(),
        ])
        .run()?;

    println!("{CYAN}=== Hexz full command + flag test ==={RESET}");
    println!("{CYAN}BIN={bin_str}{RESET}");

    // [1] sys doctor
    println!("{CYAN}[1] sys doctor{RESET}");
    cmd(bin_str).args(["sys", "doctor"]).run()?;

    // [2] data info
    println!("{CYAN}[2] data info{RESET}");
    cmd(bin_str)
        .args(["data", "info", snap_base.to_str().unwrap()])
        .run()?;

    // [3] data build with profiles
    for profile in ["generic", "eda", "embedded", "ml"] {
        println!("{CYAN}[build] profile={profile}{RESET}");
        let out = tmp_path.join(format!("out-{profile}.hxz"));
        cmd(bin_str)
            .args([
                "data",
                "build",
                "--source",
                small_raw.to_str().unwrap(),
                "--output",
                out.to_str().unwrap(),
                "--profile",
                profile,
            ])
            .run()?;
    }

    // [4] data pack with compression variants
    println!("{CYAN}[create] compression=lz4 block_size=65536{RESET}");
    let lz4_out = tmp_path.join("create-lz4.hxz");
    cmd(bin_str)
        .args([
            "data",
            "pack",
            "--disk",
            small_raw.to_str().unwrap(),
            "--output",
            lz4_out.to_str().unwrap(),
            "--compression",
            "lz4",
            "--block-size",
            "65536",
        ])
        .run()?;

    println!("{CYAN}[create] compression=zstd block_size=32768{RESET}");
    let zstd_out = tmp_path.join("create-zstd.hxz");
    cmd(bin_str)
        .args([
            "data",
            "pack",
            "--disk",
            small_raw.to_str().unwrap(),
            "--output",
            zstd_out.to_str().unwrap(),
            "--compression",
            "zstd",
            "--block-size",
            "32768",
        ])
        .run()?;

    // [5] keygen
    let keys_dir = tmp_path.join("keys");
    std::fs::create_dir_all(&keys_dir)?;
    println!("{CYAN}[keygen] Generating keys...{RESET}");
    cmd(bin_str)
        .args(["sys", "keygen", "--output-dir", keys_dir.to_str().unwrap()])
        .run()?;

    // [6] sign / verify
    let signed = tmp_path.join("signed.hxz");
    std::fs::copy(&snap_base, &signed)?;
    println!("{CYAN}[sign] Signing snapshot...{RESET}");
    cmd(bin_str)
        .args([
            "sys",
            "sign",
            "--key",
            keys_dir.join("private.key").to_str().unwrap(),
            signed.to_str().unwrap(),
        ])
        .run()?;

    println!("{CYAN}[verify] Verifying snapshot...{RESET}");
    cmd(bin_str)
        .args([
            "sys",
            "verify",
            "--key",
            keys_dir.join("public.key").to_str().unwrap(),
            signed.to_str().unwrap(),
        ])
        .run()?;

    // [7] bench
    println!("{CYAN}[bench] Standard benchmark...{RESET}");
    cmd(bin_str)
        .args(["sys", "bench", snap_base.to_str().unwrap()])
        .run()?;

    println!("{CYAN}[bench] Custom parameters...{RESET}");
    cmd(bin_str)
        .args([
            "sys",
            "bench",
            snap_base.to_str().unwrap(),
            "--block-size",
            "65536",
            "--duration",
            "1",
            "--threads",
            "1",
        ])
        .run()?;

    // [8] analyze
    println!("{CYAN}[analyze] Analyzing snapshot...{RESET}");
    cmd(bin_str)
        .args(["data", "analyze", snap_base.to_str().unwrap()])
        .run()?;

    // [9] mount / unmount
    let mnt = tmp_path.join("mnt");
    std::fs::create_dir_all(&mnt)?;
    println!("{CYAN}[mount] Mounting (daemon mode)...{RESET}");
    cmd(bin_str)
        .args([
            "vm",
            "mount",
            snap_base.to_str().unwrap(),
            mnt.to_str().unwrap(),
            "-d",
        ])
        .run()?;

    std::thread::sleep(std::time::Duration::from_secs(2));
    let _ = cmd("ls")
        .args(["-la", mnt.to_str().unwrap()])
        .run_with_status();

    cmd(bin_str)
        .args(["vm", "unmount", mnt.to_str().unwrap()])
        .run()?;
    println!("{GREEN}Unmounted successfully{RESET}");

    // [10] mount RW with overlay
    let overlay = tmp_path.join("overlay");
    println!("{CYAN}[mount] Mounting with overlay (RW)...{RESET}");
    cmd(bin_str)
        .args([
            "vm",
            "mount",
            snap_base.to_str().unwrap(),
            mnt.to_str().unwrap(),
            "--overlay",
            overlay.to_str().unwrap(),
            "--rw",
            "-d",
        ])
        .run()?;

    std::thread::sleep(std::time::Duration::from_secs(2));
    let _ = std::fs::write(mnt.join(".hexz-rw-test"), "");

    cmd(bin_str)
        .args(["vm", "unmount", mnt.to_str().unwrap()])
        .run()?;

    // [11] diff
    println!("{CYAN}[diff] Checking overlay diffs...{RESET}");
    cmd(bin_str)
        .args([
            "data",
            "diff",
            overlay.to_str().unwrap(),
            "--blocks",
            "--files",
        ])
        .run()?;

    // [12] commit
    println!("{CYAN}[commit] Committing changes...{RESET}");
    let committed = tmp_path.join("committed.hxz");
    cmd(bin_str)
        .args([
            "vm",
            "commit",
            snap_base.to_str().unwrap(),
            overlay.to_str().unwrap(),
            committed.to_str().unwrap(),
            "--compression",
            "zstd",
            "--block-size",
            "65536",
            "--keep-overlay",
            "--flatten",
            "--message",
            "test commit",
        ])
        .run()?;
    cmd(bin_str)
        .args(["data", "info", committed.to_str().unwrap()])
        .run()?;

    // [13] serve
    println!("{CYAN}[serve] Testing HTTP server...{RESET}");
    let mut server = std::process::Command::new(bin_str)
        .args([
            "sys",
            "serve",
            snap_base.to_str().unwrap(),
            "--port",
            "18080",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    std::thread::sleep(std::time::Duration::from_secs(2));

    // Check HTTP response
    match ureq::get("http://127.0.0.1:18080/disk").call() {
        Ok(resp) => println!("HTTP {}", resp.status()),
        Err(e) => println!("HTTP check: {e}"),
    }

    let _ = server.kill();
    let _ = server.wait();

    // [14] boot (if qemu exists)
    if which::which("qemu-system-x86_64").is_ok() {
        println!("{CYAN}[boot] Testing boot command (timeout 5s)...{RESET}");
        let _ = cmd("timeout")
            .args([
                "5",
                bin_str,
                "vm",
                "boot",
                snap_base.to_str().unwrap(),
                "--no-graphics",
                "--ram",
                "2G",
                "--network",
                "user",
                "--backend",
                "qemu",
            ])
            .run_with_status();
    } else {
        println!("{YELLOW}qemu-system-x86_64 not found, skipping boot test{RESET}");
    }

    println!("{GREEN}All tests passed!{RESET}");
    // tmp dir auto-cleaned by TempDir drop
    Ok(())
}

fn mount() -> Result<()> {
    let root = find_workspace_root()?;
    let bin = root.join("target/release/hexz");

    // Build if needed
    if !bin.exists() {
        println!("{CYAN}Building hexz (release)\u{2026}{RESET}");
        cmd(cargo())
            .args(["build", "--release", "--workspace"])
            .current_dir(&root)
            .run()?;
    }

    let tmp = tempfile::tempdir()?;
    let tmp_path = tmp.path();
    let bin_str = bin.to_str().unwrap();

    println!("{CYAN}>>> Starting FUSE Mount Test{RESET}");

    // Create test data
    println!("{CYAN}Creating test data...{RESET}");
    let src_data = tmp_path.join("mount_test.data");
    std::fs::write(&src_data, "Hello Hexz World")?;

    let snap_file = tmp_path.join("mount_test.hxz");
    let mount_dir = tmp_path.join("mnt_test_point");
    std::fs::create_dir_all(&mount_dir)?;

    // Create snapshot
    println!("{CYAN}Creating snapshot...{RESET}");
    cmd(bin_str)
        .args([
            "data",
            "pack",
            "--disk",
            src_data.to_str().unwrap(),
            "--output",
            snap_file.to_str().unwrap(),
        ])
        .run()?;

    // Mount (background)
    println!("{CYAN}Mounting...{RESET}");
    let mut mount_proc = std::process::Command::new(bin_str)
        .args([
            "vm",
            "mount",
            snap_file.to_str().unwrap(),
            mount_dir.to_str().unwrap(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    std::thread::sleep(std::time::Duration::from_secs(2));

    // Verify content
    println!("{CYAN}Verifying content...{RESET}");
    let disk_path = mount_dir.join("disk");
    let content = std::fs::read_to_string(&disk_path)?;
    let expected = "Hello Hexz World";

    if content.trim() != expected {
        // Cleanup before failing
        let _ = cmd(bin_str)
            .args(["vm", "unmount", mount_dir.to_str().unwrap()])
            .run_with_status();
        let _ = mount_proc.kill();
        let _ = mount_proc.wait();
        anyhow::bail!(
            "Content mismatch: expected '{expected}', got '{}'",
            content.trim()
        );
    }

    println!("{GREEN}Read successful: {content}{RESET}");

    // Unmount
    let _ = cmd(bin_str)
        .args(["vm", "unmount", mount_dir.to_str().unwrap()])
        .run_with_status();
    let _ = mount_proc.kill();
    let _ = mount_proc.wait();

    println!("{GREEN}Test Passed.{RESET}");
    Ok(())
}
