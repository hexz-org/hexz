# Hexz Scripts

This directory contains utility scripts for development, testing, and VM management. **The central entry point for all development is the Makefile at the repo root** — run **`make help`** there for build, test, lint, and setup targets. Most automation has been migrated to `cargo xtask` — run **`cargo xtask --help`** for the full list of subcommands.

## Shared Utilities

### `lib/common.sh`
Common bash functions used by benchmark scripts in `tools/bench/`.

## VM Management

### `vm_manager.py`
A Python utility for automating Hexz VM workflows.
- **Install:** Downloads Ubuntu ISOs and runs `hexz install`.
- **Boot:** Boots the created VM snapshots with configurable RAM/Disk.
- **Networking:** Handles QEMU user/tap networking flags.

**Usage:**
```bash
python3 tools/scripts/vm_manager.py install --disk-size 20G
python3 tools/scripts/vm_manager.py boot --ram 4G
python3 tools/scripts/vm_manager.py all
```

## Migrated to xtask

The following scripts have been replaced by typed Rust subcommands in `tools/xtask/`:

| Old script | New command |
|---|---|
| `run_minio.sh` | `cargo xtask minio [start\|stop\|status]` |
| `vm_test.sh` | `cargo xtask vm-test` |
| `test_all_commands.sh` | `cargo xtask test commands` |
| `test_mount.sh` | `cargo xtask test mount` |
