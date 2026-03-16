use crate::common::{
    BOLD, CYAN, GREEN, RESET, cargo, cmd, copy_dir_recursive, find_workspace_root, require_cmd,
};
use anyhow::{Result, bail};

const BENCH_PACKAGE: &str = "hexz";
const CRITERION_DIR: &str = "target/criterion";
const BENCH_STORE_DIR: &str = ".criterion";

#[derive(clap::Subcommand)]
pub enum BaselineCmd {
    /// Run benchmarks and save as a named baseline
    Save {
        /// Baseline name
        name: String,
    },
    /// Archive criterion data to `.criterion/<name>/`
    Archive {
        /// Baseline name
        name: String,
    },
    /// Restore an archived baseline into target/criterion/
    Restore {
        /// Baseline name
        name: String,
    },
    /// Compare two archived baselines using critcmp
    Compare {
        /// Old baseline name
        old: String,
        /// New baseline name
        new: String,
    },
    /// Run benchmarks, then compare to an archived baseline
    BenchCompare {
        /// Archived baseline name to compare against
        name: String,
        /// Optional filter (substring of bench name)
        filter: Option<String>,
    },
}

pub fn run(cmd: BaselineCmd) -> Result<()> {
    match cmd {
        BaselineCmd::Save { name } => save(&name),
        BaselineCmd::Archive { name } => archive(&name),
        BaselineCmd::Restore { name } => restore(&name),
        BaselineCmd::Compare { old, new } => compare(&old, &new),
        BaselineCmd::BenchCompare { name, filter } => bench_compare(&name, filter.as_deref()),
    }
}

fn save(name: &str) -> Result<()> {
    let root = find_workspace_root()?;
    println!("{GREEN}Running benchmarks and saving baseline '{name}'\u{2026}{RESET}");
    cmd(cargo())
        .args(["bench", "-p", BENCH_PACKAGE, "--", "--save-baseline", name])
        .current_dir(&root)
        .run()
}

fn archive(name: &str) -> Result<()> {
    let root = find_workspace_root()?;
    let criterion = root.join(CRITERION_DIR);
    let archive_dir = root.join(BENCH_STORE_DIR).join(name);

    if !criterion.is_dir() {
        bail!(
            "{BOLD}Error:{RESET} No criterion directory found at {CRITERION_DIR}. Run 'cargo xtask baseline save <name>' first."
        );
    }

    println!("{GREEN}Archiving baseline to {BENCH_STORE_DIR}/{name}...{RESET}");
    std::fs::create_dir_all(&archive_dir)?;
    copy_dir_recursive(&criterion, &archive_dir)?;
    println!("{CYAN}Baseline '{name}' archived to {BENCH_STORE_DIR}/{name}.{RESET}");
    Ok(())
}

fn restore(name: &str) -> Result<()> {
    let root = find_workspace_root()?;
    let archive_dir = root.join(BENCH_STORE_DIR).join(name);
    let criterion = root.join(CRITERION_DIR);

    if !archive_dir.is_dir() {
        bail!("{BOLD}Error:{RESET} Baseline archive '{name}' not found in {BENCH_STORE_DIR}.");
    }

    println!("{GREEN}Restoring baseline '{name}' from archive...{RESET}");
    if criterion.exists() {
        std::fs::remove_dir_all(&criterion)?;
    }
    std::fs::create_dir_all(&criterion)?;
    copy_dir_recursive(&archive_dir, &criterion)?;
    println!("{CYAN}Baseline '{name}' restored to {CRITERION_DIR}.{RESET}");
    Ok(())
}

fn compare(old: &str, new: &str) -> Result<()> {
    let root = find_workspace_root()?;
    require_cmd("critcmp")?;

    let criterion = root.join(CRITERION_DIR);
    std::fs::create_dir_all(&criterion)?;

    println!("{GREEN}Preparing to compare '{old}' vs '{new}'...{RESET}");

    // Copy both baselines into criterion dir (non-clobbering)
    let old_dir = root.join(BENCH_STORE_DIR).join(old);
    let new_dir = root.join(BENCH_STORE_DIR).join(new);
    if old_dir.is_dir() {
        copy_dir_recursive(&old_dir, &criterion)?;
    }
    if new_dir.is_dir() {
        copy_dir_recursive(&new_dir, &criterion)?;
    }

    println!("{GREEN}Running critcmp...{RESET}");
    cmd("critcmp").args([old, new]).current_dir(&root).run()
}

fn bench_compare(name: &str, filter: Option<&str>) -> Result<()> {
    let root = find_workspace_root()?;
    require_cmd("critcmp")?;

    let archive_dir = root.join(BENCH_STORE_DIR).join(name);
    if !archive_dir.is_dir() {
        bail!("{BOLD}Error:{RESET} Baseline '{name}' not found in {BENCH_STORE_DIR}/");
    }

    let tmp_name = "_cmp";

    println!(
        "{GREEN}Running benchmarks{} (saving as {tmp_name})\u{2026}{RESET}",
        filter
            .map(|f| format!(" matching '{f}'"))
            .unwrap_or_default()
    );

    let mut bench_cmd = cmd(cargo()).args(["bench", "--package", BENCH_PACKAGE]);
    if let Some(f) = filter {
        bench_cmd = bench_cmd.arg(f);
    }
    bench_cmd
        .args(["--", "--save-baseline", tmp_name])
        .current_dir(&root)
        .run()?;

    println!("{GREEN}Comparing to archived baseline '{name}'\u{2026}{RESET}");
    let criterion = root.join(CRITERION_DIR);
    std::fs::create_dir_all(&criterion)?;
    copy_dir_recursive(&archive_dir, &criterion)?;

    let mut cmp_cmd = cmd("critcmp").args([name, tmp_name]);
    if let Some(f) = filter {
        cmp_cmd = cmp_cmd.args(["-f", f]);
    }
    cmp_cmd.current_dir(&root).run()
}
