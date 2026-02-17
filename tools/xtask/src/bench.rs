use crate::common::*;
use anyhow::Result;
use walkdir::WalkDir;

/// Benchmark group categories matching the `[[bench]]` layout in `crates/cli/Cargo.toml`.
const MICRO_BENCHES: &[&str] = &[
    "cache_concurrent",
    "scatter_gather",
    "read_api_comparison",
    "decompress_scaling",
    "compression",
    "hashing",
    "cdc_chunking",
    "encryption",
    "hash_table",
];

const MACRO_BENCHES: &[&str] = &[
    "read_throughput",
    "sparse_access",
    "concurrency",
    "gzip_comparison",
    "write_throughput",
    "dedup_efficiency",
    "block_size_tradeoffs",
    "pack_memory",
];

const AI_BENCHES: &[&str] = &[
    "ai_dataloader",
    "ai_shuffle",
    "ai_prefetch",
    "ai_multiworker",
    "ai_tensor_ops",
    "ai_ml_workloads",
];

const HTTP_BENCHES: &[&str] = &["http_throughput"];

#[derive(clap::Subcommand)]
pub enum BenchCmd {
    /// List available benchmark categories
    List,
    /// Run benchmarks by group and profile
    Run {
        /// Benchmark group to run
        #[arg(long, default_value = "all")]
        group: BenchGroup,
        /// Benchmark profile (quick uses reduced sampling)
        #[arg(long, default_value = "full")]
        profile: BenchProfile,
    },
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum BenchGroup {
    Micro,
    Macro,
    Ai,
    Http,
    All,
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum BenchProfile {
    Quick,
    Full,
}

pub fn run(cmd: BenchCmd) -> Result<()> {
    match cmd {
        BenchCmd::List => list(),
        BenchCmd::Run { group, profile } => run_benches(group, profile),
    }
}

fn bench_names(group: &BenchGroup) -> Vec<&'static str> {
    match group {
        BenchGroup::Micro => MICRO_BENCHES.to_vec(),
        BenchGroup::Macro => MACRO_BENCHES.to_vec(),
        BenchGroup::Ai => AI_BENCHES.to_vec(),
        BenchGroup::Http => HTTP_BENCHES.to_vec(),
        BenchGroup::All => {
            let mut all = Vec::new();
            all.extend_from_slice(MICRO_BENCHES);
            all.extend_from_slice(MACRO_BENCHES);
            all.extend_from_slice(AI_BENCHES);
            all.extend_from_slice(HTTP_BENCHES);
            all
        }
    }
}

fn run_benches(group: BenchGroup, profile: BenchProfile) -> Result<()> {
    let benches = bench_names(&group);
    let quick = matches!(profile, BenchProfile::Quick);

    println!(
        "{GREEN}Running {} benchmark(s) ({:?}, profile: {:?})\u{2026}{RESET}",
        benches.len(),
        group,
        profile,
    );

    for name in &benches {
        println!("\n{CYAN}\u{25b6} {name}{RESET}");
        let mut c = cmd(cargo());
        c = c.args(["bench", "--package", "hexz-cli", "--bench", name]);
        if quick {
            c = c.args(["--", "--quick"]);
        }
        c.run()?;
    }

    println!("\n{GREEN}All benchmarks complete.{RESET}");
    Ok(())
}

fn list() -> Result<()> {
    let root = find_workspace_root()?;
    let criterion_dir = root.join("target/criterion");
    let bench_store = root.join(".criterion");

    // Find a directory with benchmark data
    let bench_dir = if criterion_dir.is_dir() {
        Some(criterion_dir)
    } else if bench_store.is_dir() {
        let mut found = None;
        if let Ok(entries) = std::fs::read_dir(&bench_store) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    found = Some(entry.path());
                    break;
                }
            }
        }
        found
    } else {
        None
    };

    // Show criterion data if available
    if let Some(bench_dir) = bench_dir {
        println!(
            "{GREEN}Benchmark categories (use with make bench <category> or bench-compare <baseline> <category>)\u{2026}{RESET}\n"
        );

        let mut categories = std::collections::BTreeSet::new();
        let prefix = bench_dir.to_string_lossy().to_string();

        for entry in WalkDir::new(&bench_dir).into_iter().flatten() {
            if entry.file_name() == "benchmark.json" {
                let path = entry.path().to_string_lossy();
                if let Some(relative) = path.strip_prefix(&prefix) {
                    let relative = relative.trim_start_matches('/');
                    if let Some(category) = relative.split('/').next() {
                        categories.insert(category.to_string());
                    }
                }
            }
        }

        for cat in &categories {
            println!("{cat}");
        }
        println!();
    }

    // Always show static groups
    print_static_groups();

    Ok(())
}

fn print_static_groups() {
    println!("{GREEN}Benchmark groups:{RESET}\n");
    println!(
        "  {BOLD}micro{RESET}  ({} benches): {}",
        MICRO_BENCHES.len(),
        MICRO_BENCHES.join(", ")
    );
    println!(
        "  {BOLD}macro{RESET}  ({} benches): {}",
        MACRO_BENCHES.len(),
        MACRO_BENCHES.join(", ")
    );
    println!(
        "  {BOLD}ai{RESET}     ({} benches): {}",
        AI_BENCHES.len(),
        AI_BENCHES.join(", ")
    );
    println!(
        "  {BOLD}http{RESET}   ({} benches): {}",
        HTTP_BENCHES.len(),
        HTTP_BENCHES.join(", ")
    );
    println!("\n  Use: cargo xtask bench run --group <group> [--profile quick]");
}
