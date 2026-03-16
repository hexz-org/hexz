use crate::common::{GREEN, RESET, cargo, cmd, find_workspace_root, require_cmd};
use anyhow::Result;

const CROSS_AARCH64: &str = "aarch64-unknown-linux-gnu";
const CROSS_WINDOWS: &str = "x86_64-pc-windows-gnu";
const AARCH64_CLI_FEAT: &str = "server,compression-zstd,encryption,diagnostics,signing,s3";
const WINDOWS_CLI_FEAT: &str = "compression-zstd,encryption,diagnostics,signing,s3";
const AARCH64_LINKER: &str = "aarch64-linux-gnu-gcc";
const WINDOWS_LINKER: &str = "x86_64-w64-mingw32-gcc";

#[derive(Clone, Copy, clap::Subcommand)]
pub enum CrossTarget {
    /// Cross-check for aarch64-unknown-linux-gnu
    Aarch64,
    /// Cross-check for x86_64-pc-windows-gnu
    Windows,
    /// Cross-check all targets
    All,
}

pub fn run(target: CrossTarget) -> Result<()> {
    match target {
        CrossTarget::Aarch64 => check_aarch64(),
        CrossTarget::Windows => check_windows(),
        CrossTarget::All => {
            check_aarch64()?;
            check_windows()
        }
    }
}

fn check_aarch64() -> Result<()> {
    let root = find_workspace_root()?;
    require_cmd(AARCH64_LINKER)?;

    println!("\n{GREEN}[cross] Checking {CROSS_AARCH64} (binary)\u{2026}{RESET}");
    cmd(cargo())
        .args([
            "check",
            "-p",
            "hexz-cli",
            "--target",
            CROSS_AARCH64,
            "--no-default-features",
            "--features",
            AARCH64_CLI_FEAT,
        ])
        .env(
            &format!("CC_{}", CROSS_AARCH64.replace('-', "_")),
            AARCH64_LINKER,
        )
        .env(
            "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER",
            AARCH64_LINKER,
        )
        .env(
            &format!("CFLAGS_{}", CROSS_AARCH64.replace('-', "_")),
            "-D__ARM_ARCH=8",
        )
        .current_dir(&root)
        .run()?;

    println!("{GREEN}[cross] Checking {CROSS_AARCH64} (wheel)\u{2026}{RESET}");
    cmd(cargo())
        .args(["check", "-p", "hexz-loader", "--target", CROSS_AARCH64])
        .env(
            &format!("CC_{}", CROSS_AARCH64.replace('-', "_")),
            AARCH64_LINKER,
        )
        .env(
            "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER",
            AARCH64_LINKER,
        )
        .env(
            &format!("CFLAGS_{}", CROSS_AARCH64.replace('-', "_")),
            "-D__ARM_ARCH=8",
        )
        .current_dir(&root)
        .run()
}

fn check_windows() -> Result<()> {
    let root = find_workspace_root()?;
    require_cmd(WINDOWS_LINKER)?;

    println!("\n{GREEN}[cross] Checking {CROSS_WINDOWS} (binary)\u{2026}{RESET}");
    cmd(cargo())
        .args([
            "check",
            "-p",
            "hexz-cli",
            "--target",
            CROSS_WINDOWS,
            "--no-default-features",
            "--features",
            WINDOWS_CLI_FEAT,
        ])
        .env("CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER", WINDOWS_LINKER)
        .env(
            &format!("CC_{}", CROSS_WINDOWS.replace('-', "_")),
            WINDOWS_LINKER,
        )
        .current_dir(&root)
        .run()?;

    println!("{GREEN}[cross] Checking {CROSS_WINDOWS} (wheel)\u{2026}{RESET}");
    cmd(cargo())
        .args(["check", "-p", "hexz-loader", "--target", CROSS_WINDOWS])
        .env("CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER", WINDOWS_LINKER)
        .env(
            &format!("CC_{}", CROSS_WINDOWS.replace('-', "_")),
            WINDOWS_LINKER,
        )
        .current_dir(&root)
        .run()
}
