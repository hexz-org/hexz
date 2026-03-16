use crate::common::{CYAN, GREEN, RESET, cmd};
use anyhow::Result;

const CONTAINER_NAME: &str = "hexz-minio";
const DEFAULT_PORT: &str = "9000";
const DEFAULT_CONSOLE_PORT: &str = "9001";
const DEFAULT_USER: &str = "minioadmin";
const DEFAULT_PASSWORD: &str = "minioadmin";
const DEFAULT_BUCKET: &str = "hexz-test";
const DEFAULT_DATA_DIR: &str = "/tmp/hexz-minio-data";

#[derive(Clone, Copy, clap::Subcommand)]
pub enum MinioCmd {
    /// Start `MinIO` container
    Start,
    /// Stop and remove `MinIO` container
    Stop,
    /// Show `MinIO` status and connection info
    Status,
}

pub fn run(cmd: MinioCmd) -> Result<()> {
    match cmd {
        MinioCmd::Start => start(),
        MinioCmd::Stop => {
            stop();
            Ok(())
        }
        MinioCmd::Status => {
            status();
            Ok(())
        }
    }
}

fn env_or(var: &str, default: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| default.into())
}

fn is_running() -> bool {
    cmd("docker")
        .args(["ps", "--format", "{{.Names}}"])
        .capture()
        .is_ok_and(|out| out.lines().any(|line| line.trim() == CONTAINER_NAME))
}

fn start() -> Result<()> {
    let port = env_or("MINIO_PORT", DEFAULT_PORT);
    let console_port = env_or("MINIO_CONSOLE_PORT", DEFAULT_CONSOLE_PORT);
    let user = env_or("MINIO_ROOT_USER", DEFAULT_USER);
    let password = env_or("MINIO_ROOT_PASSWORD", DEFAULT_PASSWORD);
    let bucket = env_or("BUCKET_NAME", DEFAULT_BUCKET);
    let data_dir = env_or("MINIO_DATA_DIR", DEFAULT_DATA_DIR);

    if is_running() {
        println!("{CYAN}[minio]{RESET} MinIO is already running");
        status();
        return Ok(());
    }

    // Clean up stopped container
    let _ = cmd("docker")
        .args(["rm", "-f", CONTAINER_NAME])
        .run_with_status();

    println!("{CYAN}[minio]{RESET} Starting MinIO on :{port}\u{2026}");
    std::fs::create_dir_all(&data_dir)?;

    cmd("docker")
        .args([
            "run",
            "-d",
            "--name",
            CONTAINER_NAME,
            "-p",
            &format!("{port}:9000"),
            "-p",
            &format!("{console_port}:9001"),
            "-e",
            &format!("MINIO_ROOT_USER={user}"),
            "-e",
            &format!("MINIO_ROOT_PASSWORD={password}"),
            "-v",
            &format!("{data_dir}:/data"),
            "minio/minio:latest",
            "server",
            "/data",
            "--console-address",
            ":9001",
        ])
        .run()?;

    // Wait for MinIO to be ready
    println!("{CYAN}[minio]{RESET} Waiting for MinIO to start\u{2026}");
    let health_url = format!("http://localhost:{port}/minio/health/live");
    for _ in 0..30 {
        match ureq::get(&health_url).call() {
            Ok(_) => break,
            Err(_) => std::thread::sleep(std::time::Duration::from_secs(1)),
        }
    }

    // Create test bucket if mc is available
    if which::which("mc").is_ok() {
        let _ = cmd("mc")
            .args([
                "alias",
                "set",
                "hexz-local",
                &format!("http://localhost:{port}"),
                &user,
                &password,
            ])
            .run_with_status();

        let _ = cmd("mc")
            .args(["mb", &format!("hexz-local/{bucket}")])
            .run_with_status();

        println!("{GREEN}[minio]{RESET} Test bucket '{bucket}' ready");
    } else {
        println!("{CYAN}[minio]{RESET} Install 'mc' (MinIO Client) to auto-create buckets");
    }

    println!("{GREEN}[minio]{RESET} MinIO running");
    status();
    Ok(())
}

fn stop() {
    println!("{CYAN}[minio]{RESET} Stopping MinIO\u{2026}");
    let _ = cmd("docker")
        .args(["stop", CONTAINER_NAME])
        .run_with_status();
    let _ = cmd("docker").args(["rm", CONTAINER_NAME]).run_with_status();
    println!("{GREEN}[minio]{RESET} MinIO stopped");
}

fn status() {
    let port = env_or("MINIO_PORT", DEFAULT_PORT);
    let console_port = env_or("MINIO_CONSOLE_PORT", DEFAULT_CONSOLE_PORT);
    let user = env_or("MINIO_ROOT_USER", DEFAULT_USER);
    let password = env_or("MINIO_ROOT_PASSWORD", DEFAULT_PASSWORD);

    if is_running() {
        println!("{GREEN}[minio]{RESET} MinIO is running");
        println!("  API:     http://localhost:{port}");
        println!("  Console: http://localhost:{console_port}");
        println!("  User:    {user}");
        println!();
        println!("  export AWS_ENDPOINT_URL=http://localhost:{port}");
        println!("  export AWS_ACCESS_KEY_ID={user}");
        println!("  export AWS_SECRET_ACCESS_KEY={password}");
    } else {
        println!("{CYAN}[minio]{RESET} MinIO is not running");
    }
}
