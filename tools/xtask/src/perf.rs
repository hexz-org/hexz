use crate::common::*;
use anyhow::Result;

#[derive(clap::Subcommand)]
pub enum PerfCmd {
    /// Profile Rust CLI data pack via samply
    Rust {
        /// Size of test data in MB
        #[arg(long, default_value = "256")]
        size_mb: u32,
    },
    /// Remove profiling artifacts
    Clean,
}

pub fn run(cmd: PerfCmd) -> Result<()> {
    match cmd {
        PerfCmd::Rust { size_mb } => rust(size_mb),
        PerfCmd::Clean => clean(),
    }
}

fn rust(size_mb: u32) -> Result<()> {
    let root = find_workspace_root()?;
    require_cmd("samply")?;

    println!("{GREEN}Building Rust CLI (release + frame pointers)\u{2026}{RESET}");
    cmd(cargo())
        .args(["build", "--release", "--package", "hexz-cli"])
        .env("RUSTFLAGS", "-C force-frame-pointers=yes")
        .current_dir(&root)
        .run()?;

    let perf_dir = std::path::PathBuf::from("/tmp/hexz_perf");
    std::fs::create_dir_all(&perf_dir)?;

    println!("{GREEN}Generating {size_mb}MB of test data\u{2026}{RESET}");
    cmd("dd")
        .args([
            "if=/dev/urandom",
            &format!("of={}/test.bin", perf_dir.display()),
            "bs=1M",
            &format!("count={size_mb}"),
        ])
        .run()?;

    println!("{GREEN}Profiling hexz data pack (samply, {size_mb}MB)\u{2026}{RESET}");
    cmd("samply")
        .args([
            "record",
            "--",
            root.join("target/release/hexz").to_str().unwrap(),
            "data",
            "pack",
            "--input",
            &format!("{}/test.bin", perf_dir.display()),
            "--output",
            &format!("{}/output.hxz", perf_dir.display()),
            "--silent",
        ])
        .current_dir(&root)
        .run()?;

    let _ = std::fs::remove_dir_all(&perf_dir);
    println!(
        "\n{CYAN}Tip: type {BOLD}hexz{RESET}{CYAN} in the search box to highlight only hexz frames{RESET}"
    );
    Ok(())
}

fn clean() -> Result<()> {
    println!("{GREEN}Cleaning profiling artifacts\u{2026}{RESET}");
    let root = find_workspace_root()?;
    let _ = std::fs::remove_dir_all(root.join("tools/perf/results"));
    let _ = std::fs::remove_dir_all("/tmp/hexz_perf");
    Ok(())
}
