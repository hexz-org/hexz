# CLI Command Reference

Complete reference for the `hexz` command-line tool.

## Installation

```bash
# Build from source
git clone https://github.com/Alethic-Systems/hexz.git
cd hexz
make rust

# Binary location
./target/release/hexz
```

## Command Structure

```
hexz <CATEGORY> <COMMAND> [OPTIONS]
```

**Categories**:
- `data` — Dataset and snapshot operations
- `vm` — Virtual machine management
- `sys` — System utilities and diagnostics

---

## Data Commands

### `hexz data pack`

Pack files or directories into a compressed snapshot.

**Synopsis**:
```bash
hexz data pack --disk <INPUT> --output <OUTPUT> [OPTIONS]
```

**Options**:
- `--disk <PATH>` — Input file or directory (required)
- `--output <PATH>` — Output snapshot path (required)
- `--compression <ALGO>` — Compression algorithm: `lz4` (default) or `zstd`
- `--compression-level <N>` — Zstd compression level (1-22, default: 3)
- `--block-size <BYTES>` — Block size (default: 65536)
- `--cdc` — Enable content-defined chunking for deduplication
- `--parent <PATH>` — Parent snapshot for incremental packing
- `--encrypt` — Enable AES-256-GCM encryption
- `--key <PATH>` — Encryption key file

**Example**:
```bash
hexz data pack \\
  --disk /data/imagenet \\
  --output imagenet.hxz \\
  --compression zstd \\
  --compression-level 9 \\
  --cdc
```

### `hexz data info`

Display snapshot metadata.

**Synopsis**:
```bash
hexz data info <SNAPSHOT>
```

**Output**:
```
Snapshot: dataset.hxz
Format Version: 1
Compression: Zstandard (Level 9)
Block Size: 65536
Uncompressed Size: 1.2 GB
Compressed Size: 456 MB
Compression Ratio: 2.63×
Block Count: 18750
Deduplication: Enabled
Encrypted: No
Signed: Yes
```

### `hexz data diff`

Compare two snapshots.

**Synopsis**:
```bash
hexz data diff <SNAPSHOT1> <SNAPSHOT2>
```

**Output**: List of changed blocks with offsets.

---

## VM Commands

### `hexz vm boot`

Boot a virtual machine from a snapshot.

**Synopsis**:
```bash
hexz vm boot <SNAPSHOT> [OPTIONS]
```

**Resource Options**:
- `--ram <SIZE>` — RAM allocation (e.g., `4G`, default: `2G`)
- `--cpus <N>` — Number of virtual CPUs (default: 2)

**Network Options**:
- `--net` — Enable user-mode networking
- `--forward <HOST:GUEST>` — Port forwarding (e.g., `8080:80`)

**Display Options**:
- `--vnc` — Enable VNC server on port 5900
- `--headless` — No display output

**Advanced**:
- `--snapshot` — Ephemeral mode (discard changes on exit)
- `--kernel-mode` — Boot in kernel development mode

**Example**:
```bash
hexz vm boot ubuntu.hxz \\
  --ram 4G \\
  --cpus 4 \\
  --net \\
  --forward 2222:22 \\
  --forward 8080:80
```

### `hexz vm install`

Install OS from ISO and save as snapshot.

**Synopsis**:
```bash
hexz vm install --iso <ISO> --output <SNAPSHOT> [OPTIONS]
```

**Options**:
- `--iso <PATH>` — Installation ISO (required)
- `--output <PATH>` — Output snapshot path (required)
- `--disk-size <SIZE>` — Virtual disk size (default: `20G`)
- `--ram <SIZE>` — RAM allocation (default: `2G`)
- `--vnc` — Enable VNC for interactive installation

**Example**:
```bash
hexz vm install \\
  --iso ubuntu-22.04.iso \\
  --output ubuntu.hxz \\
  --disk-size 40G \\
  --ram 4G \\
  --vnc
```

### `hexz vm mount`

Mount snapshot as FUSE filesystem.

**Synopsis**:
```bash
hexz vm mount <SNAPSHOT> <MOUNTPOINT> [OPTIONS]
```

**Options**:
- `--overlay <PATH>` — Enable write support with overlay file
- `--readonly` — Mount read-only (default)

**Example**:
```bash
# Read-only mount
hexz vm mount dataset.hxz /mnt/hexz --readonly

# Read-write with overlay
hexz vm mount base.hxz /mnt/hexz --overlay changes.img
```

### `hexz vm commit`

Commit overlay changes to new snapshot.

**Synopsis**:
```bash
hexz vm commit --base <BASE> --overlay <OVERLAY> --output <OUTPUT>
```

**Example**:
```bash
hexz vm commit \\
  --base ubuntu-base.hxz \\
  --overlay changes.img \\
  --output ubuntu-updated.hxz
```

---

## System Commands

### `hexz sys doctor`

Diagnose system configuration.

**Synopsis**:
```bash
hexz sys doctor
```

**Checks**:
- QEMU installation and version
- KVM support and permissions
- FUSE support
- Library versions
- File system capabilities

### `hexz sys bench`

Run performance benchmarks.

**Synopsis**:
```bash
hexz sys bench [OPTIONS]
```

**Options**:
- `--compression <ALGO>` — Test specific algorithm (lz4, zstd, or all)
- `--block-size <BYTES>` — Block size for tests
- `--threads <N>` — Number of threads

**Example**:
```bash
hexz sys bench --compression all --threads 8
```

### `hexz sys keygen`

Generate Ed25519 signing keypair.

**Synopsis**:
```bash
hexz sys keygen [--output-dir <DIR>]
```

**Output**: Creates `private.key` and `public.key` in specified directory (default: current directory).

### `hexz sys sign`

Sign a snapshot.

**Synopsis**:
```bash
hexz sys sign --key <PRIVATE_KEY> <SNAPSHOT>
```

**Example**:
```bash
hexz sys sign --key private.key dataset.hxz
```

### `hexz sys verify`

Verify snapshot signature.

**Synopsis**:
```bash
hexz sys verify --key <PUBLIC_KEY> <SNAPSHOT>
```

**Exit Codes**:
- `0`: Signature valid
- `1`: Signature invalid or missing

---

## Global Options

These options work with all commands:

- `-h, --help` — Show help message
- `-V, --version` — Show version
- `-v, --verbose` — Increase logging verbosity (can be repeated: `-vv`, `-vvv`)
- `--quiet` — Suppress non-error output

---

## Environment Variables

- `HEXZ_CACHE_DIR` — Default cache directory for remote snapshots
- `HEXZ_CACHE_SIZE` — Default cache size in bytes
- `AWS_PROFILE` — AWS profile for S3 access
- `AWS_DEFAULT_REGION` — AWS region for S3

---

## Exit Codes

- `0` — Success
- `1` — General error
- `2` — Invalid arguments
- `3` — Permission denied
- `4` — File not found
- `5` — I/O error

---

## See Also

- [How-To: Pack Datasets](../how-to/cli-usage/pack-datasets.md)
- [How-To: Install Hexz](../how-to/cli-usage/install-hexz.md)
- [Tutorial: Getting Started](../tutorials/getting-started.md)
