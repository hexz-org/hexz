# hexz-cli

Command-line tool for managing Hexz high-performance deduplicated archives.

## Overview

The `hexz` CLI provides a comprehensive interface for creating, analyzing, and managing Hexz archives (`.hxz`). It is optimized for large-scale data distribution, offering block-level deduplication, transparent compression, and instant random-access via FUSE mounting.

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/Alethic-Systems/hexz.git
cd hexz

# Install the CLI
make install

# Or install directly with cargo
cargo install --path crates/cli
```

## Quick Examples

### Pack an Archive

Convert files or directories into a compressed, deduplicated Hexz archive:

```bash
# Pack a directory
hexz pack ./data dataset.hxz --compression zstd

# Create a thin delta against a base archive
hexz pack ./data_v2 update.hxz --base base.hxz
```

### Mount an Archive

Access archive contents instantly without extraction (requires FUSE):

```bash
mkdir /mnt/hexz
hexz mount archive.hxz /mnt/hexz
ls -lh /mnt/hexz
```

### Extract an Archive

```bash
# Extract an archive to a directory
hexz extract archive.hxz ./output
```

### Inspect an Archive

```bash
# Show archive metadata and stats
hexz show archive.hxz

# Get JSON output for programmatic access
hexz show archive.hxz --json
```

## Command Reference

### Archive & Filesystem Operations

| Command | Description |
|---------|-------------|
| `pack` | Create an archive from a file or directory |
| `extract` | Reconstruct original files from an archive |
| `mount` | Mount an archive as a FUSE filesystem |
| `unmount` | Safely detach a mounted archive |

### Inspection & Comparison

| Command | Description |
|---------|-------------|
| `show` | Display archive metadata and statistics |
| `diff` | Compare two archives at the block level |
| `log` | List archives in a directory and show their lineage |

## Common Options

### Compression

Choose compression algorithm with `--compression`:
- `lz4` - Fast compression (~2GB/s), lower ratio (default)
- `zstd` - Better compression (~500MB/s), higher ratio

### Deduplication (CDC)

Hexz uses Content-Defined Chunking (CDC) to identify shared blocks even if offsets shift. This is enabled by default.

### Encryption

Encrypt archives with `--encrypt`:
- Uses AES-256-GCM.
- You'll be prompted for a password (or use `HEXZ_PASSWORD` env var).

## Features

The CLI supports compile-time feature flags:

- `default`: `["fuse", "server", "compression-zstd", "encryption", "signing"]`
- `fuse`: FUSE filesystem support for mounting
- `server`: HTTP server for archive streaming
- `compression-zstd`: Zstandard compression
- `encryption`: AES-256-GCM encryption
- `signing`: Cryptographic signing for archives

## Performance

The CLI is optimized for high-throughput operations:

- **Pack throughput**: ~2GB/s (LZ4), ~500MB/s (Zstd)
- **Random Access**: O(1) seek time via hierarchical indexing
- **Deduplication**: FastCDC with parallel processing
- **Zero-copy**: Direct memory mapping for efficient reads

## See Also

- **[hexz-core](../core/)** - Core engine library
- **[Project README](../../README.md)** - Main project overview
