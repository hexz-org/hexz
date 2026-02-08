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

### Prerequisites
* Rust: Stable toolchain.
* Python: 3.10 or higher.
* Build Tools:
    * maturin (for Python wheels): pip install maturin
    * libfuse-dev (Linux only, for VM support)
    * qemu (Optional, for VM tests)

### Setup Steps
1. Clone the repo:
   ```bash
   git clone https://github.com/willmccallion/strata.git
   cd strata
   ```

2. Build the Rust workspace to verify everything compiles:
   ```bash
   cargo build
   cargo test --no-run
   ```

3. (Optional) Set up Python development environment for AI loader:
   ```bash
   python3 -m venv .venv
   source .venv/bin/activate  # On Windows: .venv\Scripts\activate
   pip install maturin torch pytest pillow numpy tqdm
   ```

4. (Optional) Build the Python loader:
   ```bash
   maturin develop --manifest-path crates/loader/Cargo.toml
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
Use this for packing datasets.

# Build release binary
cargo build --release --bin strata

# Run the binary
./target/release/strata --help

### 2. Building the Python Library (AI Loader)
Use this to test the PyTorch integration.

# Must have .venv activated
maturin develop --manifest-path crates/loader/Cargo.toml --release

# Verify installation
python -c "import strata; print(strata.__version__)"

---

## Testing

### Rust Unit Tests (Core Logic)
Test compression and file format logic.

# Run all tests
cargo test

# Run core tests only
cargo test -p strata-core

### Python Integration Tests (AI Loader)
Test that the loader works with PyTorch.

# 1. Build latest version
maturin develop --manifest-path crates/loader/Cargo.toml

# 2. Run Python tests
pytest tests/python/

### VM Boot Tests (Systems)
Requires Linux and QEMU.

./scripts/test_vm_boot.sh

---

## Style Guide

### Rust
Code must be formatted.
cargo fmt
cargo clippy -- -D warnings

### Python
We use ruff or black.
pip install ruff
ruff check .

---

## Pull Request Checklist

Before submitting to dev:
1. ✅ Builds pass: `cargo build --all-features`
2. ✅ Tests pass: `cargo test --all-features`
3. ✅ Code is formatted: `cargo fmt --all`
4. ✅ Lints pass: `cargo clippy --all-features -- -D warnings`
5. ✅ No large binary files or test artifacts committed
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
- Use `cargo test <test_name>` to run specific tests

### Documentation
- Use `///` doc comments for public APIs
- Run `cargo doc --open` to build and view documentation locally
- Examples in doc comments should be runnable (`cargo test --doc`)
