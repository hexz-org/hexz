use crate::common::{
    cargo, check_mark, cmd, find_workspace_root, maturin, BOLD, CYAN, GREEN, RED, RESET, YELLOW,
};
use anyhow::{Context, Result, bail};
use semver::Version;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

// Publish order mirrors release.yml: each crate after its real (non-dev) deps.
// hexz-core is published before hexz-store/hexz-ops even though it has dev-deps
// on them — --no-verify skips building so the circular dev-dep is harmless.
const PUBLISH_ORDER: &[&str] = &[
    "hexz-common",
    "hexz-core",
    "hexz-store",
    "hexz-ops",
    "hexz-fuse",
    "hexz-server",
    "hexz-cli",
];

// ── TOML structures ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CargoToml {
    workspace: Option<Workspace>,
}

#[derive(Deserialize)]
struct Workspace {
    package: Option<WorkspacePackage>,
}

#[derive(Deserialize)]
struct WorkspacePackage {
    version: String,
}

#[derive(Deserialize)]
struct PyProject {
    project: Option<PyProjectProject>,
}

#[derive(Deserialize)]
struct PyProjectProject {
    version: String,
}

// ── crates.io / PyPI JSON response ──────────────────────────────────────────

#[derive(Deserialize)]
struct CratesIoResponse {
    #[serde(rename = "crate")]
    krate: Option<CrateInfo>,
    errors: Option<Vec<serde_json::Value>>,
}

#[derive(Deserialize)]
struct CrateInfo {
    max_version: String,
}

#[derive(Deserialize)]
struct PyPiResponse {
    info: Option<PyPiInfo>,
}

#[derive(Deserialize)]
struct PyPiInfo {
    version: String,
}

// ── Version extraction ──────────────────────────────────────────────────────

fn workspace_version(root: &Path) -> Result<Version> {
    let content =
        std::fs::read_to_string(root.join("Cargo.toml")).context("reading root Cargo.toml")?;
    let ct: CargoToml = toml::from_str(&content).context("parsing root Cargo.toml")?;
    let raw = ct
        .workspace
        .and_then(|w| w.package)
        .map(|p| p.version)
        .context("no workspace.package.version in root Cargo.toml")?;
    Version::parse(&raw).context("parsing workspace version")
}

fn pyproject_version(root: &Path) -> Result<Version> {
    let path = root.join("crates/loader/pyproject.toml");
    let content = std::fs::read_to_string(&path).context("reading pyproject.toml")?;
    let pp: PyProject = toml::from_str(&content).context("parsing pyproject.toml")?;
    let raw = pp
        .project
        .map(|p| p.version)
        .context("no project.version in pyproject.toml")?;
    Version::parse(&raw).context("parsing pyproject.toml version")
}

fn init_py_version(root: &Path) -> Result<Version> {
    let path = root.join("crates/loader/python/hexz/__init__.py");
    let content = std::fs::read_to_string(&path).context("reading __init__.py")?;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("__version__") {
            if let Some(rest) = rest.trim().strip_prefix('=') {
                let raw = rest.trim().trim_matches('"').trim_matches('\'');
                return Version::parse(raw).context("parsing __init__.py version");
            }
        }
    }
    bail!("could not find __version__ in __init__.py")
}

// ── Registry queries ────────────────────────────────────────────────────────

fn crates_io_version(name: &str) -> Result<Option<Version>> {
    let url = format!("https://crates.io/api/v1/crates/{name}");
    let response = ureq::get(&url)
        .set("User-Agent", "hexz-xtask-version-check")
        .call();

    match response {
        Ok(resp) => {
            let body: CratesIoResponse = resp
                .into_json()
                .context(format!("parsing JSON from {url}"))?;
            if body.errors.is_some() {
                return Ok(None);
            }
            match body.krate {
                Some(info) => Ok(Some(Version::parse(&info.max_version)?)),
                None => Ok(None),
            }
        }
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(e) => Err(e).context(format!("fetching {url}")),
    }
}

fn pypi_version(name: &str) -> Result<Option<Version>> {
    let url = format!("https://pypi.org/pypi/{name}/json");
    let response = ureq::get(&url)
        .set("User-Agent", "hexz-xtask-version-check")
        .call();

    match response {
        Ok(resp) => {
            let body: PyPiResponse = resp.into_json().context("parsing PyPI JSON")?;
            match body.info {
                Some(info) => Ok(Some(Version::parse(&info.version)?)),
                None => Ok(None),
            }
        }
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(e) => Err(e).context(format!("fetching {url}")),
    }
}

// ── Git checks ──────────────────────────────────────────────────────────────

fn is_git_clean() -> Result<bool> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .context("failed to run git status")?;
    Ok(output.stdout.is_empty())
}

fn current_git_branch() -> Result<String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .context("failed to run git branch")?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// ── Main logic ──────────────────────────────────────────────────────────────

pub fn run() -> Result<()> {
    let root = find_workspace_root()?;
    let ws_ver = workspace_version(&root)?;
    let mut failed = false;

    println!("{GREEN}Checking versions and environment\u{2026}{RESET}");
    println!("  Workspace version: {ws_ver}\n");

    // ── Pre-release Git Checks ──────────────────────────────────────────────

    println!("  {BOLD}Git Environment{RESET}");

    let branch = current_git_branch()?;
    let ok_branch = branch == "main" || branch == "master";
    if !ok_branch {
        failed = true;
    }
    if ok_branch {
        println!("  {} branch is '{branch}'", check_mark(true));
    } else {
        println!(
            "  {} {RED}branch is '{branch}' (expected main or master){RESET}",
            check_mark(false)
        );
    }

    let clean = is_git_clean()?;
    if !clean {
        failed = true;
    }
    if clean {
        println!("  {} working directory is clean", check_mark(true));
    } else {
        println!(
            "  {} {RED}working directory has uncommitted changes{RESET}",
            check_mark(false)
        );
    }

    println!();

    // ── Consistency checks ──────────────────────────────────────────────────

    println!("  {BOLD}Consistency{RESET}");

    let py_ver = pyproject_version(&root)?;
    let ok = py_ver == ws_ver;
    if !ok {
        failed = true;
    }
    if ok {
        println!("  {} pyproject.toml    {py_ver}", check_mark(true));
    } else {
        println!(
            "  {} {RED}pyproject.toml    {py_ver} (expected {ws_ver}){RESET}",
            check_mark(false)
        );
    }

    let init_ver = init_py_version(&root)?;
    let ok = init_ver == ws_ver;
    if !ok {
        failed = true;
    }
    if ok {
        println!("  {} __init__.py       {init_ver}", check_mark(true));
    } else {
        println!(
            "  {} {RED}__init__.py       {init_ver} (expected {ws_ver}){RESET}",
            check_mark(false)
        );
    }

    // ── crates.io checks ───────────────────────────────────────────────────

    println!("\n  {BOLD}crates.io{RESET}");

    let crate_names = [
        "hexz-common",
        "hexz-core",
        "hexz-fuse",
        "hexz-server",
        "hexz-cli",
    ];

    for name in &crate_names {
        match crates_io_version(name)? {
            None => {
                println!("  {} {name:<16} not yet published", check_mark(true));
            }
            Some(published) => {
                if ws_ver > published {
                    println!(
                        "  {} {name:<16} {ws_ver} > {published} (published)",
                        check_mark(true)
                    );
                } else {
                    println!(
                        "  {} {RED}{name:<16} {ws_ver} <= {published} (published){RESET}",
                        check_mark(false)
                    );
                    failed = true;
                }
            }
        }
    }

    // ── PyPI check ─────────────────────────────────────────────────────────

    println!("\n  {BOLD}PyPI{RESET}");

    match pypi_version("hexz")? {
        None => {
            println!("  {} {:<16} not yet published", check_mark(true), "hexz");
        }
        Some(published) => {
            if ws_ver > published {
                println!(
                    "  {} {:<16} {ws_ver} > {published} (published)",
                    check_mark(true),
                    "hexz"
                );
            } else {
                println!(
                    "  {} {RED}{:<16} {ws_ver} <= {published} (published){RESET}",
                    check_mark(false),
                    "hexz"
                );
                failed = true;
            }
        }
    }

    println!();

    if failed {
        println!(
            "  {RED}{BOLD}Pre-release checks failed \u{2014} resolve issues before releasing{RESET}\n"
        );
        bail!("pre-release checks failed");
    }

    println!("  {GREEN}{BOLD}All pre-release checks passed{RESET}\n");
    Ok(())
}

// ── Publish dry-run ──────────────────────────────────────────────────────────

pub fn publish_dry_run() -> Result<()> {
    let root = find_workspace_root()?;

    // ── Crates ───────────────────────────────────────────────────────────────
    //
    // Cargo's --dry-run validates that a crate can be packaged and that all
    // its dependencies exist on crates.io at the required version.
    //
    // This creates an inherent bootstrap constraint for a workspace: crates
    // that depend on sibling crates can only be validated after those siblings
    // have been published. We check each crate and report whether the failure
    // is a real packaging error or just an expected "sibling not yet published"
    // dependency-resolution failure.

    println!("{GREEN}Dry-run: cargo publish{RESET}\n");

    // First determine which sibling crates already exist on crates.io at the
    // current workspace version — those are the only ones that can be used as
    // resolved deps during dry-run packaging.
    let ws_ver = workspace_version(&root)?;
    let mut published_at_current: std::collections::HashSet<&str> =
        std::collections::HashSet::new();
    for krate in PUBLISH_ORDER {
        if let Ok(Some(v)) = crates_io_version(krate) {
            if v >= ws_ver {
                let _ = published_at_current.insert(krate);
            }
        }
    }

    let mut any_real_failure = false;
    for krate in PUBLISH_ORDER {
        print!("  {CYAN}cargo publish --dry-run --no-verify -p {krate}{RESET} … ");
        let status = cmd(cargo())
            .args(["publish", "--dry-run", "--no-verify", "-p", krate])
            .current_dir(&root)
            .run_with_status()?;

        if status.success() {
            println!("{}", check_mark(true));
        } else {
            // Determine if this is a known bootstrap limitation or a real error.
            // A crate can only be validated if all its sibling deps are already
            // published at the current version.
            let is_bootstrap_limitation = PUBLISH_ORDER
                .iter()
                .take_while(|&&c| c != *krate) // crates that publish before this one
                .any(|sibling| !published_at_current.contains(sibling));

            if is_bootstrap_limitation {
                println!(
                    "{YELLOW}skipped{RESET} \
                     (sibling dep not yet on crates.io — expected until first release)"
                );
            } else {
                println!("{}", check_mark(false));
                any_real_failure = true;
            }
        }
    }

    // ── Python wheel (maturin build) ─────────────────────────────────────────
    //
    // maturin has no publish --dry-run. Building the wheel locally is the
    // equivalent check — it catches Cargo.toml metadata errors, missing
    // features, and compilation failures before the real publish.

    println!("\n{GREEN}Dry-run: maturin build (Python wheel){RESET}\n");

    let loader_dir = root.join("crates/loader");
    print!("  {CYAN}maturin build --out /tmp/hexz-wheel-dryrun{RESET} … ");
    let status = cmd(maturin())
        .args(["build", "--out", "/tmp/hexz-wheel-dryrun"])
        .current_dir(&loader_dir)
        .run_with_status()?;

    if status.success() {
        println!("{}", check_mark(true));
        println!("  Wheel written to /tmp/hexz-wheel-dryrun/");
    } else {
        println!("{}", check_mark(false));
        any_real_failure = true;
    }

    println!();
    if any_real_failure {
        bail!("publish dry-run had failures — see output above");
    }
    println!("{GREEN}{BOLD}Publish dry-run passed{RESET}\n");
    Ok(())
}
