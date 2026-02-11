# strata-cli

Command-line tool for managing Strata snapshots, datasets, and virtual machines.

## Overview

The `strata` CLI provides a comprehensive interface for creating, analyzing, and managing Strata snapshots. It supports dataset packing for AI/ML workflows, VM snapshot management, and diagnostic tools for inspecting snapshot internals.

This is the primary tool for developers and data engineers working with the Strata format.

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/willmccallion/strata.git
cd strata

# Install the CLI
make install

# Or install directly with cargo
cargo install --path crates/cli
```

After installation, the `strata` command will be available in your PATH.

## Quick Examples

### Pack a Dataset

Convert raw files into a compressed, deduplicated Strata snapshot:

```bash
# Pack a directory of images for ML training
strata data pack --disk ./raw_images --output dataset.st --cdc

# Pack with custom compression and encryption
strata data pack \
  --disk ./data \
  --output encrypted.st \
  --compression zstd \
  --encrypt
```

### Inspect a Snapshot

```bash
# Show snapshot metadata
strata data info dataset.st

# Get JSON output for programmatic access
strata data info dataset.st --json
```

### Boot a VM from Snapshot

```bash
# Boot a VM with 4GB RAM (requires FUSE feature)
strata vm boot ubuntu-22.04.st --ram 4G

# Boot without KVM acceleration
strata vm boot snapshot.st --ram 2G --no-kvm
```

## Command Reference

The CLI is organized into three main command groups:

### Data Commands (`strata data`)

Work with datasets and snapshots for ML/AI workflows:

| Command | Description |
|---------|-------------|
| `pack` | Create a snapshot from raw files/directories |
| `build` | Build a snapshot with specific profiles (optimized for different workloads) |
| `info` | Display snapshot metadata and statistics |
| `diff` | Compare two snapshots (diagnostics feature) |
| `analyze` | Analyze snapshot structure and compression efficiency (diagnostics feature) |

**Example:**
```bash
# Pack with deduplication
strata data pack --disk ./images --output train.st --cdc

# View detailed info
strata data info train.st --json
```

### VM Commands (`strata vm`)

Manage virtual machines using Strata snapshots (requires `fuse` feature):

| Command | Description |
|---------|-------------|
| `boot` | Boot a VM from a snapshot |
| `install` | Install an OS from ISO to create a new snapshot |
| `snapshot` | Capture running VM state (disk + memory) |
| `mount` | Mount a snapshot as a FUSE filesystem |

**Example:**
```bash
# Install Ubuntu from ISO
strata vm install ubuntu-22.04.iso --disk-size 20G --output ubuntu.st

# Boot the installed system
strata vm boot ubuntu.st --ram 4G

# Snapshot a running VM
strata vm snapshot --socket /tmp/vm.sock --output checkpoint.st
```

### System Commands (`strata sys`)

Server and infrastructure operations:

| Command | Description |
|---------|-------------|
| `serve` | Start an HTTP server for streaming snapshots |

**Example:**
```bash
# Serve snapshots over HTTP
strata sys serve --port 8080 --path /snapshots
```

## Common Options

### Compression

Choose compression algorithm with `--compression`:
- `lz4` - Fast compression (~2GB/s), lower ratio (default)
- `zstd` - Better compression (~500MB/s), higher ratio

```bash
strata data pack --disk ./data --output data.st --compression zstd
```

### Content-Defined Chunking (CDC)

Enable deduplication with `--cdc`:

```bash
# Use default CDC settings (FastCDC)
strata data pack --disk ./data --output data.st --cdc

# Custom chunk sizes
strata data pack \
  --disk ./data \
  --output data.st \
  --cdc \
  --min-chunk 16384 \
  --avg-chunk 65536 \
  --max-chunk 262144
```

### Encryption

Encrypt snapshots with `--encrypt`:

```bash
# You'll be prompted for a password
strata data pack --disk ./data --output secure.st --encrypt
```

## Architecture

```
strata-cli/
├── src/
│   ├── main.rs        # Entry point
│   ├── args.rs        # CLI argument parsing (clap)
│   ├── cmd/           # Command implementations
│   │   ├── data/      # Dataset commands (pack, info, etc.)
│   │   ├── vm/        # VM commands (boot, snapshot, etc.)
│   │   └── sys/       # System commands (serve)
│   └── ui/            # User interface (progress bars, formatters)
└── benches/           # Performance benchmarks
    ├── macro/         # Macro benchmarks (end-to-end)
    ├── micro/         # Micro benchmarks (component-level)
    └── ai/            # AI/ML-focused benchmarks
```

## Development

All development commands use the project Makefile from the repository root.

### Building

```bash
# Build CLI (release mode)
make rust

# Build and install locally
make install

# Run CLI directly
make run info dataset.st
```

### Testing

```bash
# Run all tests
make test

# Run only Rust tests
make test-rust

# Run tests with filter
make test-rust pack
```

### Benchmarks

The CLI crate includes comprehensive benchmarks:

```bash
# Run all benchmarks
make bench

# Run specific benchmark category
make bench ai           # AI/ML workload benchmarks
make bench cache        # Cache performance
make bench http         # HTTP streaming

# Compare against baseline
make bench-compare baseline-v1
```

Benchmark categories:
- **macro**: End-to-end workflows (read throughput, sparse access, concurrency)
- **micro**: Component-level (cache, decompression, API comparison)
- **ai**: ML-specific (dataloader, shuffle, prefetch, multi-worker)

### Linting & Formatting

```bash
# Format all code
make fmt

# Check formatting + clippy
make lint

# Run clippy with strict lints
make clippy
```

See `make help` for all available commands.

## Features

The CLI supports compile-time feature flags:

- `default`: `["fuse", "server", "compression-zstd", "encryption", "diagnostics", "signing"]`
- `fuse`: VM mounting and FUSE filesystem support
- `server`: HTTP server for snapshot streaming
- `compression-zstd`: Zstandard compression
- `encryption`: AES-256-GCM encryption
- `diagnostics`: Advanced analysis commands (diff, analyze)
- `signing`: Cryptographic signing for snapshots
- `firecracker`: Firecracker microVM support (experimental)

Build without optional features:

```bash
# Minimal build (no VM support)
cargo build -p strata --no-default-features --features encryption,compression-zstd
```

## Performance

The CLI is optimized for high-throughput operations:

- **Pack throughput**: ~2GB/s (LZ4), ~500MB/s (Zstd)
- **Deduplication**: FastCDC with parallel processing
- **Progress tracking**: Real-time progress bars with `indicatif`
- **Zero-copy**: Direct memory mapping where possible

## See Also

- **[User Documentation](../../docs/)** - Tutorials and how-to guides
- **[CLI Reference](../../docs/reference/cli-reference.md)** - Complete command documentation
- **[strata-core](../core/)** - Core engine library
- **[Python Bindings](../loader/)** - PyTorch integration
- **[Project README](../../README.md)** - Main project overview
