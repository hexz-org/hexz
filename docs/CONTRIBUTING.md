# Contributing to Strata

Welcome to the Strata project.

Strata is a high-performance system designed for two distinct use cases:
1. AI Infrastructure: A streaming data loader for PyTorch.
2. Systems Virtualization: A seekable FUSE filesystem for VM booting.

Please read this guide to understand the development workflow and project structure.

---

## Git Workflow & Branching

We use a Stable Main workflow.

### The Branches
1. main: Production-ready code. Do not push directly to this branch. It is updated only via Pull Requests from dev.
2. dev: The integration branch. All feature branches merge here first.
3. feature/<name>: Your working branch for new tasks (e.g., feature/async-s3).

### The Workflow
1. Create a branch: git checkout -b feature/my-feature
2. Write your code.
3. Run tests.
4. Open a Pull Request targeting the dev branch.
5. Merge into dev once CI passes.

---

## Development Setup

All setup, build, and test commands go through the **Makefile** at the repo root. There are no separate setup scripts or one-off commands to remember.

### Prerequisites (checked by `make setup`)

Run **`make setup-check`** from the repo root. It will list any missing system packages and show install commands for your OS. You need:

* **Rust** — rustup + cargo (https://rustup.rs)
* **pkg-config** — for C library detection
* **Python 3** — for the AI loader and docs
* **libfuse** — dev headers (Linux: libfuse-dev or fuse2/fuse3; macOS: macFUSE via Homebrew)
* **qemu** — optional, only for VM boot tests

### Setup (one command)

1. Clone the repo and run the central setup from the repo root:
   ```bash
   git clone https://github.com/willmccallion/strata.git
   cd strata
   make setup
   ```
   This installs Rust components (rustfmt, clippy), cargo tools (cargo-deny, maturin, etc.), and creates a Python venv with docs requirements. If anything is missing, run **`make setup-check`** first and install what it suggests.

2. Verify the workspace and (optionally) install the Python loader for development:
   ```bash
   make check      # type-check
   make develop    # editable Python package for AI loader
   ```

---

## Project Architecture

Strata is a Rust Cargo Workspace.

| Directory | Crate Name | Description |
| :--- | :--- | :--- |
| crates/core | strata-core | File format, compression, and deduplication logic. Shared by all crates. |
| crates/loader | strata-loader | PyO3 bindings for the AI data loader. Contains S3 streaming logic. |
| crates/cli | strata-cli | The command-line tool for packing datasets and managing VMs. |
| crates/fuse | strata-fuse | FUSE adapter for mounting Strata filesystems (VM support). |
| crates/server | strata-server | HTTP server for streaming data. |

---

## Building & Running

### 1. Building the CLI Tool (Rust)

From repo root:

```bash
make rust
./target/release/strata --help
```

### 2. Building the Python Library (AI Loader)

From repo root (Makefile handles venv and maturin):

```bash
make develop
python -c "import strata; print(strata.__version__)"
```

---

## Testing

### Rust Unit Tests (Core Logic)

From repo root:

```bash
make test-rust
```

To run tests for a single crate only: `cargo test -p strata-core` (the Makefile does not define per-crate targets).

### Python Integration Tests (AI Loader)

From repo root:

```bash
make test-python
```

### VM Boot Tests (Systems)

Requires Linux and QEMU. From repo root:

```bash
./scripts/test_vm_boot.sh
```

---

## Style Guide

### Rust & Python

From repo root:

```bash
make fmt      # Format code
make lint     # Format check + clippy + ruff
```

---

## Pull Request Checklist

Before submitting to dev:
1. ✅ Run full CI pipeline locally: `make ci`
2. ✅ No large binary files or test artifacts committed
6. ✅ Update relevant documentation if adding new features
7. ✅ Pull Request title is descriptive and follows conventional commits style

---

## Getting Help

- Check the [docs/](../docs/) directory for technical documentation
- Look at existing examples in [examples/](../examples/)
- Open an issue if you find a bug or have questions
- Read the [ROADMAP.md](ROADMAP.md) to see planned features

---

## Code Organization Tips

### Adding New Features
1. **Core logic** goes in `crates/core/` (file format, compression, algorithms)
2. **Python bindings** go in `crates/loader/` (PyO3 wrappers)
3. **CLI commands** go in `crates/cli/src/commands/`
4. **Examples** should be self-contained in `examples/<name>/`

### Writing Tests
- Unit tests go in the same file as the code (Rust convention)
- Integration tests go in `crates/*/tests/`
- Test fixtures are in `crates/core/tests/common/`
- **Primary:** `make test` (Rust + Python). For a single test: `cargo test <test_name>`.

### Documentation
- Use `///` doc comments for public APIs
- **Primary:** `make docs` (rustdoc) and `make docs-python` (Sphinx). For a single crate: `cargo doc --open -p <crate>`.
- Examples in doc comments should be runnable (`cargo test --doc`)
