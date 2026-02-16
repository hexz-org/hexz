use anyhow::{Context, Result, bail};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

// ── Colors ──────────────────────────────────────────────────────────────────

pub const GREEN: &str = "\x1b[32m";
pub const RED: &str = "\x1b[31m";
pub const CYAN: &str = "\x1b[36m";
pub const YELLOW: &str = "\x1b[33m";
pub const BOLD: &str = "\x1b[1m";
pub const RESET: &str = "\x1b[0m";

pub fn check_mark(ok: bool) -> &'static str {
    if ok {
        "\x1b[32m\u{2713}\x1b[0m"
    } else {
        "\x1b[31m\u{2717}\x1b[0m"
    }
}

// ── Workspace root discovery ────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct CargoToml {
    workspace: Option<Workspace>,
}

#[derive(serde::Deserialize)]
struct Workspace {
    package: Option<WorkspacePackage>,
}

#[derive(serde::Deserialize)]
struct WorkspacePackage {
    #[allow(dead_code)]
    version: String,
}

pub fn find_workspace_root() -> Result<PathBuf> {
    let start = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap());

    let mut dir = start.as_path();
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = std::fs::read_to_string(&cargo_toml)?;
            if let Ok(parsed) = content.parse::<toml::Table>() {
                if parsed.contains_key("workspace") {
                    let ct: CargoToml = toml::from_str(&content)?;
                    if ct
                        .workspace
                        .as_ref()
                        .and_then(|w| w.package.as_ref())
                        .is_some()
                    {
                        return Ok(dir.to_path_buf());
                    }
                }
            }
        }
        dir = dir
            .parent()
            .context("reached filesystem root without finding workspace Cargo.toml")?;
    }
}

// ── Tool discovery ──────────────────────────────────────────────────────────

pub fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".into())
}

pub fn python(root: &Path) -> String {
    let venv_python3 = root.join(".venv/bin/python3");
    if venv_python3.exists() {
        return venv_python3.display().to_string();
    }
    let venv_python = root.join(".venv/bin/python");
    if venv_python.exists() {
        return venv_python.display().to_string();
    }
    "python3".into()
}

pub fn maturin() -> String {
    std::env::var("MATURIN").unwrap_or_else(|_| "maturin".into())
}

pub fn mkdocs(root: &Path) -> String {
    let venv_mkdocs = root.join(".venv/bin/mkdocs");
    if venv_mkdocs.exists() {
        return venv_mkdocs.display().to_string();
    }
    "mkdocs".into()
}

pub fn require_cmd(name: &str) -> Result<()> {
    if which::which(name).is_ok() {
        return Ok(());
    }
    let hint = match name {
        "rustup" => "Install from https://rustup.rs",
        "cargo" => "Install Rust from https://rustup.rs",
        "pkg-config" => "apt install pkg-config / pacman -S pkg-config / brew install pkg-config",
        "python3" => "apt install python3 / pacman -S python / brew install python",
        "samply" => "cargo install samply",
        "critcmp" => "cargo install critcmp",
        "cargo-llvm-cov" => "cargo install cargo-llvm-cov",
        "docker" => "Install Docker: https://docs.docker.com/get-docker/",
        "qemu-system-x86_64" => "apt install qemu-system-x86 / pacman -S qemu-full",
        "mc" => "Install MinIO Client: https://min.io/docs/minio/linux/reference/minio-mc.html",
        _ => "Check your PATH or install the tool",
    };
    bail!("{BOLD}Missing:{RESET} {name}\n  {CYAN}{hint}{RESET}")
}

// ── Command builder ─────────────────────────────────────────────────────────

pub struct Cmd {
    inner: Command,
    display: String,
}

impl Cmd {
    pub fn env(mut self, key: &str, val: &str) -> Self {
        self.inner.env(key, val);
        self
    }

    pub fn arg<S: AsRef<OsStr>>(mut self, arg: S) -> Self {
        self.inner.arg(arg);
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.inner.args(args);
        self
    }

    pub fn current_dir<P: AsRef<Path>>(mut self, dir: P) -> Self {
        self.inner.current_dir(dir);
        self
    }

    /// Run the command, inheriting stdout/stderr. Fail on non-zero exit.
    pub fn run(mut self) -> Result<()> {
        let status = self
            .inner
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("{CYAN}failed to execute:{RESET} {}", self.display))?;
        if !status.success() {
            bail!(
                "{CYAN}command failed (exit {}): {RESET}{}",
                status.code().unwrap_or(-1),
                self.display
            );
        }
        Ok(())
    }

    /// Run the command and capture stdout as a String.
    pub fn capture(mut self) -> Result<String> {
        let output = self
            .inner
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .output()
            .with_context(|| format!("{CYAN}failed to execute:{RESET} {}", self.display))?;
        if !output.status.success() {
            bail!(
                "{CYAN}command failed (exit {}): {RESET}{}",
                output.status.code().unwrap_or(-1),
                self.display
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Run the command and return the exit status (don't fail on non-zero).
    pub fn run_with_status(mut self) -> Result<ExitStatus> {
        self.inner
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("{CYAN}failed to execute:{RESET} {}", self.display))
    }

    /// Run the command, capturing both stdout and stderr. Fail on non-zero exit.
    pub fn capture_all(mut self) -> Result<String> {
        let output = self
            .inner
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .with_context(|| format!("{CYAN}failed to execute:{RESET} {}", self.display))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "{CYAN}command failed (exit {}): {RESET}{}\n{}",
                output.status.code().unwrap_or(-1),
                self.display,
                stderr
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// Create a command builder that streams output by default.
pub fn cmd<S: AsRef<OsStr>>(program: S) -> Cmd {
    let display = program.as_ref().to_string_lossy().into_owned();
    Cmd {
        inner: Command::new(program),
        display,
    }
}

// ── Filesystem helpers ──────────────────────────────────────────────────────

pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }
    for entry in
        std::fs::read_dir(src).with_context(|| format!("reading directory {}", src.display()))?
    {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}
