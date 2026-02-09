# Strata Scripts

This directory contains utility scripts for development, testing, and VM management.

## Development Utilities

### `setup_dev.sh`
One-command setup for the development environment.
- Installs Rust toolchain (stable).
- Installs Python virtual environment and dependencies.
- Installs required system packages (Linux/macOS).
- Verifies workspace compilation.

**Usage:**
```bash
./scripts/setup_dev.sh
```

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
