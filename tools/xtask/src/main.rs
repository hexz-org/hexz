use clap::{Parser, Subcommand};

mod baseline;
mod bench;
mod clean;
mod common;
mod coverage;
mod cross;
mod docs;
mod minio;
mod perf;
mod setup;
mod test;
mod version;
mod vm_test;

#[derive(Parser)]
#[command(name = "xtask", about = "Hexz development automation tasks")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check version consistency and compare against published versions
    VersionCheck,

    /// Check / install development dependencies
    #[command(subcommand)]
    Setup(setup::SetupCmd),

    /// Run tests
    #[command(subcommand)]
    Test(test::TestCmd),

    /// Generate coverage reports
    #[command(subcommand)]
    Coverage(coverage::CoverageCmd),

    /// Benchmark utilities
    #[command(subcommand)]
    Bench(bench::BenchCmd),

    /// Manage criterion baselines
    #[command(subcommand)]
    Baseline(baseline::BaselineCmd),

    /// Cross-compilation checks
    CrossCheck {
        #[command(subcommand)]
        target: cross::CrossTarget,
    },

    /// Build documentation (rustdoc + mkdocs)
    Docs,

    /// Remove build artifacts
    Clean,

    /// Performance profiling via samply
    #[command(subcommand)]
    Perf(perf::PerfCmd),

    /// QEMU VM boot integration test
    VmTest,

    /// Manage local MinIO (S3-compatible) server
    #[command(subcommand)]
    Minio(minio::MinioCmd),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::VersionCheck => version::run(),
        Command::Setup(cmd) => setup::run(cmd),
        Command::Test(cmd) => test::run(cmd),
        Command::Coverage(cmd) => coverage::run(cmd),
        Command::Bench(cmd) => bench::run(cmd),
        Command::Baseline(cmd) => baseline::run(cmd),
        Command::CrossCheck { target } => cross::run(target),
        Command::Docs => docs::run(),
        Command::Clean => clean::run(),
        Command::Perf(cmd) => perf::run(cmd),
        Command::VmTest => vm_test::run(),
        Command::Minio(cmd) => minio::run(cmd),
    }
}
