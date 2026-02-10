# Strata Scripts

This directory contains utility scripts for development, testing, and VM management. **The central entry point for all development is the Makefile at the repo root** — run **`make help`** there for build, test, lint, and setup targets. The scripts below complement the Makefile.

## Development Utilities

### `setup_dev.sh`
Calls **`make setup`** from the repo root. Prefer running from repo root:

```bash
make setup
```

**`make setup`** installs Rust components, cargo tools, and a Python venv; run **`make setup-check`** first to see required system packages (rustup, pkg-config, libfuse, python) and install any that are missing.

### `run.sh`
Helper script to build and run the Strata CLI in one step.
- Automatically builds `release` binary if needed (quietly).
- Passes all arguments to the `strata` binary.

**Usage:**
```bash
./scripts/run.sh --help
./scripts/run.sh list
```

## Testing Utilities

### `run_minio.sh`
Manages a local MinIO (S3-compatible) server for integration testing.
- Starts a Docker container with MinIO.
- Creates a default test bucket (`strata-test`).
- Exports environment variables for easy connection.

**Usage:**
```bash
./scripts/run_minio.sh start
./scripts/run_minio.sh status
./scripts/run_minio.sh stop
```

## VM Management

### `vm_manager.py`
A Python utility for automating Strata VM workflows.
- **Install:** Downloads Ubuntu ISOs and runs `strata install`.
- **Boot:** Boots the created VM snapshots with configurable RAM/Disk.
- **Networking:** Handles QEMU user/tap networking flags.

**Usage:**
```bash
# Install Ubuntu (downloads ISO automatically)
python3 scripts/vm_manager.py install --disk-size 20G

# Boot the installed VM
python3 scripts/vm_manager.py boot --ram 4G

# Run both install and boot
python3 scripts/vm_manager.py all
```
