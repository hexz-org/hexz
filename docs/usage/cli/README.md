# Strata CLI — Command-Line Interface

The `strata` command-line tool provides snapshot management for virtual machines, disk images, and datasets. It uses a noun-verb command structure for intuitive operation.

## Installation

```bash
# From source
cargo install --path crates/cli

# Or build release binary
cargo build --release -p strata-cli
# Binary will be at: target/release/strata
```

## Command Structure

```
strata <CATEGORY> <COMMAND> [OPTIONS]
```

**Categories**:
- `data` — Dataset and image packing operations
- `vm` — Virtual machine snapshot management
- `sys` — System utilities and diagnostics

## Quick Reference

```bash
# Pack a disk image
strata data pack --disk image.img --output snapshot.st

# Pack with compression
strata data pack --disk image.img --output snapshot.st --compression zstd

# Inspect snapshot metadata
strata data info snapshot.st

# Boot a VM from snapshot
strata vm boot snapshot.st

# Mount snapshot as FUSE filesystem
strata vm mount snapshot.st /mnt/point

# Performance benchmarks
strata sys bench

# System diagnostics
strata sys doctor
```

---

## Data Commands

### `strata data pack`

**Purpose**: Create Strata snapshots from disk images, memory dumps, or both. This is the primary way to create `.st` files.

```bash
strata data pack [OPTIONS] --output <FILE>
```

#### Options

**Required**:
- `--output <FILE>` — Output snapshot path (`.st` extension recommended)

**Input Sources** (at least one required):
- `--disk <FILE>` — Path to disk image (raw, qcow2, etc.)
- `--memory <FILE>` — Path to memory dump file

**Compression**:
- `--compression <ALGO>` — Compression algorithm
  - `lz4` (default): Fast, ~2-3x compression, ~2GB/s decompression
  - `zstd`: Better compression (~3-5x), ~500MB/s decompression
- `--block-size <BYTES>` — Block size for compression (default: 65536)
  Smaller = better random access, larger = better compression ratio

**Deduplication**:
- `--cdc` — Enable content-defined chunking (finds duplicate chunks)
- `--min-chunk <BYTES>` — Minimum chunk size for CDC (default: 16384)
- `--avg-chunk <BYTES>` — Average chunk size for CDC (default: 65536)
- `--max-chunk <BYTES>` — Maximum chunk size for CDC (default: 131072)

**Security**:
- `--encrypt` — Enable AES-256-GCM encryption
- `--password <STRING>` — Encryption password (prompted if not provided)

#### Examples

```bash
# Basic: Pack a disk image with default settings
strata data pack \\
  --disk ubuntu-20.04.img \\
  --output ubuntu-base.st

# High compression for archival
strata data pack \\
  --disk archive.img \\
  --output archive.st \\
  --compression zstd \\
  --block-size 262144

# Pack VM snapshot (disk + memory)
strata data pack \\
  --disk vm-disk.img \\
  --memory vm-memory.dump \\
  --output vm-snapshot.st

# Encrypted snapshot
strata data pack \\
  --disk sensitive.img \\
  --output secure.st \\
  --encrypt \\
  --password "strong-passphrase"

# Deduplication for redundant data
strata data pack \\
  --disk dataset.img \\
  --output dataset.st \\
  --cdc \\
  --compression zstd

# Fast random-access snapshot (small blocks)
strata data pack \\
  --disk database.img \\
  --output db.st \\
  --block-size 16384 \\
  --compression lz4
```

#### Performance Considerations

**Block Size Impact**:
```
| Block Size | Compression | Random Access | Use Case              |
|------------|-------------|---------------|-----------------------|
| 16 KB      | Poor        | Excellent     | Databases, small I/O  |
| 64 KB      | Good        | Good          | General purpose (rec) |
| 256 KB     | Excellent   | Poor          | Sequential access     |
```

**Compression Trade-offs**:
```
| Algorithm | Speed       | Ratio | CPU Usage | Best For              |
|-----------|-------------|-------|-----------|------------------------|
| lz4       | 2000 MB/s   | 2-3x  | Low       | Training, live access  |
| zstd      | 500 MB/s    | 3-5x  | Medium    | Archival, distribution |
```

### `strata data info`

**Purpose**: Display snapshot metadata including size, compression, format version, and thin snapshot parent references.

```bash
strata data info <SNAPSHOT>
```

#### Output

```
Snapshot: dataset.st
Version: 1
Block Size: 65536 bytes (64 KB)
Compression: Lz4
Encrypted: No
Disk Size: 10737418240 bytes (10.00 GB uncompressed)
Memory Size: 0 bytes
File Size: 3489660928 bytes (3.25 GB on disk)
Compression Ratio: 3.08x
Parent: None
```

#### Examples

```bash
# Basic info
strata data info snapshot.st

# Check thin snapshot parent
strata data info incremental-v2.st
# Output will show:
#   Parent: /path/to/base-v1.st

# Verify encryption
strata data info secure.st
# Output will show:
#   Encrypted: Yes (AES-256-GCM)
```

### `strata data diff`

**Purpose**: Analyze overlay files to show modified blocks and estimated size before committing.

```bash
strata data diff <OVERLAY-FILE>
```

**Note**: Overlay files are created by FUSE mounts with `--overlay` option and have a corresponding `.meta` file tracking modified blocks.

#### Output

```
Modified Blocks: 1234
Estimated Size: 5058560 bytes (4.83 MB)
```

#### Example

```bash
# Mount with overlay
strata vm mount base.st /mnt --overlay changes.img

# ... make modifications ...

# Check what changed before committing
strata data diff changes.img
```

---

## VM Commands

### `strata vm boot`

**Purpose**: Boot a QEMU virtual machine directly from a Strata snapshot without extraction.

```bash
strata vm boot <SNAPSHOT> [OPTIONS]
```

#### Options

**Resources**:
- `--ram <SIZE>` — RAM allocation (default: 2G)
  Examples: `4G`, `8192M`, `512M`
- `--cpus <N>` — Number of virtual CPUs (default: 2)

**Networking**:
- `--net` — Enable user-mode networking (default: disabled)
- `--forward <HOST:GUEST>` — Port forwarding
  Example: `--forward 8080:80` forwards host:8080 to guest:80

**Display**:
- `--vnc` — Enable VNC server on :0 (port 5900)
- `--headless` — No display output (implies no VNC)

**Advanced**:
- `--kernel-mode` — Boot in kernel development mode (no BIOS)
- `--snapshot` — Run in snapshot mode (discards all changes on exit)

#### Examples

```bash
# Basic boot with defaults
strata vm boot ubuntu.st

# Development VM with port forwarding
strata vm boot dev.st \\
  --ram 4G \\
  --cpus 4 \\
  --net \\
  --forward 8080:80 \\
  --forward 2222:22

# Headless server
strata vm boot server.st \\
  --ram 8G \\
  --cpus 8 \\
  --headless \\
  --net \\
  --forward 8080:80

# VNC access for GUI
strata vm boot desktop.st \\
  --ram 4G \\
  --vnc

# Ephemeral testing (changes discarded)
strata vm boot test.st --snapshot
```

#### Port Forwarding Examples

```bash
# SSH access
strata vm boot vm.st --net --forward 2222:22
# Then: ssh -p 2222 user@localhost

# Web server
strata vm boot web.st --net --forward 8080:80
# Then: curl http://localhost:8080

# Multiple services
strata vm boot multi.st --net \\
  --forward 2222:22 \\   # SSH
  --forward 8080:80 \\   # HTTP
  --forward 8443:443     # HTTPS
```

### `strata vm install`

**Purpose**: Bootstrap a new VM from installation media and save as snapshot.

```bash
strata vm install [OPTIONS] --iso <FILE> --output <SNAPSHOT>
```

#### Options

**Required**:
- `--iso <FILE>` — Path to installation ISO
- `--output <SNAPSHOT>` — Output snapshot path

**VM Configuration**:
- `--disk-size <SIZE>` — Virtual disk size (default: 20G)
- `--ram <SIZE>` — RAM allocation (default: 2G)

**Installation Behavior**:
- `--vnc` — Enable VNC for interactive installation
- `--wait` — Wait for installation to complete before saving

#### Examples

```bash
# Install Ubuntu Server (interactive via VNC)
strata vm install \\
  --iso ubuntu-22.04-server.iso \\
  --output ubuntu-base.st \\
  --disk-size 40G \\
  --ram 4G \\
  --vnc

# Then connect via VNC client to localhost:5900
# Complete installation, then shutdown VM
# Snapshot is automatically saved to ubuntu-base.st

# Automated install (preseed/kickstart)
strata vm install \\
  --iso ubuntu-autoinstall.iso \\
  --output ubuntu-automated.st \\
  --disk-size 20G \\
  --wait
```

### `strata vm snap`

**Purpose**: Create a snapshot of a running VM (requires QMP socket).

```bash
strata vm snap --qmp <SOCKET> --output <SNAPSHOT>
```

**Note**: The VM must be started with QMP enabled:
```bash
qemu-system-x86_64 ... -qmp unix:/tmp/qmp.sock,server,nowait
```

#### Example

```bash
# Snapshot running VM
strata vm snap \\
  --qmp /tmp/qmp.sock \\
  --output running-state.st

# The VM is paused during snapshot, then automatically resumed
```

### `strata vm commit`

**Purpose**: Merge overlay changes back into a new snapshot, creating an incremental version.

```bash
strata vm commit [OPTIONS] --base <SNAPSHOT> --overlay <FILE> --output <NEW-SNAPSHOT>
```

#### Options

**Required**:
- `--base <SNAPSHOT>` — Original base snapshot
- `--overlay <FILE>` — Overlay file with modifications
- `--output <SNAPSHOT>` — Output snapshot path

**Optional**:
- `--memory <FILE>` — Include memory dump in new snapshot
- `--thin` — Create thin snapshot (references base for unchanged blocks)
- `--compression <ALGO>` — Compression (default: lz4)
- `--encrypt` — Encrypt the new snapshot

#### Examples

```bash
# Standard commit (full snapshot)
strata vm commit \\
  --base ubuntu-base.st \\
  --overlay changes.img \\
  --output ubuntu-updated.st

# Thin snapshot (space-efficient)
strata vm commit \\
  --base base.st \\
  --overlay delta.img \\
  --output version-2.st \\
  --thin

# Commit with memory state
strata vm commit \\
  --base base.st \\
  --overlay changes.img \\
  --memory mem.dump \\
  --output hibernated.st

# Encrypt the result
strata vm commit \\
  --base plain.st \\
  --overlay changes.img \\
  --output secure.st \\
  --encrypt
```

#### Thin Snapshots

**Thin snapshots** only store blocks that differ from the base, dramatically reducing storage:

```bash
# Create base snapshot
strata data pack --disk base.img --output base.st

# Mount with overlay
strata vm mount base.st /mnt --overlay changes.img

# ... make small modifications ...

# Create thin snapshot (only stores changes)
strata vm commit \\
  --base base.st \\
  --overlay changes.img \\
  --output v2.st \\
  --thin

# Result:
#   base.st:  10 GB (full disk)
#   v2.st:    500 MB (only changes + metadata)
#
# Reading v2.st automatically reads unchanged blocks from base.st
```

**Requirements**:
- Base snapshot must be accessible when reading thin snapshot
- Base snapshot path is embedded in thin snapshot header
- Moving base snapshot requires updating thin snapshot references (future feature)

### `strata vm mount`

**Purpose**: Mount snapshot as FUSE filesystem for browsing or modification.

```bash
strata vm mount <SNAPSHOT> <MOUNTPOINT> [OPTIONS]
```

#### Options

**Modification**:
- `--overlay <FILE>` — Enable write mode with overlay file
- `--rw` — Alias for `--overlay` (auto-generates overlay filename)

**Performance**:
- `--read-ahead <BYTES>` — Readahead buffer size (default: 131072)
- `--cache-size <BLOCKS>` — Block cache size (default: 1024)

**Alternative Protocols**:
- `--nbd` — Export as Network Block Device instead of FUSE

#### Examples

```bash
# Read-only mount
sudo strata vm mount snapshot.st /mnt/snap

# Browse files
ls -la /mnt/snap
cat /mnt/snap/etc/hostname

# Unmount
sudo umount /mnt/snap

# Read-write mount with overlay
sudo strata vm mount base.st /mnt/work --overlay changes.img

# Make modifications
echo "modified" > /mnt/work/test.txt

# Changes are tracked in changes.img and changes.img.meta
# Unmount
sudo umount /mnt/work

# Commit changes
strata vm commit --base base.st --overlay changes.img --output updated.st

# NBD export (for remote access)
sudo strata vm mount snapshot.st /dev/nbd0 --nbd

# Access as block device
sudo mount /dev/nbd0 /mnt/snap
```

---

## System Commands

### `strata sys doctor`

**Purpose**: Diagnose system configuration and dependencies.

```bash
strata sys doctor
```

#### Checks

- QEMU installation and version
- KVM support and permissions
- FUSE support
- Required libraries (libfuse, etc.)
- File system capabilities
- Network configuration

#### Output

```
✓ QEMU found: /usr/bin/qemu-system-x86_64 (v7.2.0)
✓ KVM available: /dev/kvm
✗ KVM permissions: User not in kvm group
  Fix: sudo usermod -a -G kvm $(whoami)
✓ FUSE support: OK
✓ Library versions: OK
! File system: /tmp is tmpfs (volatile)
  Warning: Snapshots in /tmp will be lost on reboot
```

### `strata sys bench`

**Purpose**: Run performance benchmarks for compression, decompression, and I/O.

```bash
strata sys bench [OPTIONS]
```

#### Options

- `--compression <ALGO>` — Test specific compression (lz4, zstd, or all)
- `--block-size <BYTES>` — Block size for tests
- `--threads <N>` — Number of threads to test

#### Output

```
Compression Benchmark
====================================================
Algorithm  Compression   Decompression   Ratio
----------------------------------------------------
LZ4        1893 MB/s     3421 MB/s       2.31x
ZSTD       487 MB/s      891 MB/s        3.78x

Block Size Impact (LZ4)
====================================================
Size       Compression   Decompression   Ratio
----------------------------------------------------
16 KB      2103 MB/s     3819 MB/s       2.11x
64 KB      1893 MB/s     3421 MB/s       2.31x
256 KB     1654 MB/s     3012 MB/s       2.52x
```

### `strata sys keygen`

**Purpose**: Generate Ed25519 keypair for signing snapshots.

```bash
strata sys keygen [--output-dir <DIR>]
```

If `--output-dir` is omitted, keys are created in the current directory as `private.key` and `public.key`.

#### Output

```
Generating Ed25519 keypair...
Keys generated:
  Private: /path/to/private.key
  Public:  /path/to/public.key
Keep the private key safe!
```

---

### Signing workflow

Snapshots can be signed so consumers can verify integrity and origin before use.

#### 1. Generate keys (once)

**CLI:**
```bash
strata sys keygen --output-dir ./keys
# Creates keys/private.key and keys/public.key
```

**Python:**
```python
import strata
priv_path, pub_path = strata.keygen(output_dir="./keys")
```

#### 2. Pack and sign a snapshot

**CLI:**
```bash
# Pack the snapshot
strata data pack --disk image.img --output release.st

# Sign it (writes signature into the .st file)
strata sys sign --key keys/private.key release.st
```

**Python:**
```python
# After building a snapshot (e.g. with strata.build() or Writer):
strata.sign_image("release.st", "keys/private.key")
```

#### 3. Verify before use

**CLI:**
```bash
strata sys verify --key keys/public.key release.st
# Succeeds silently if signature is valid; exits with error if missing or invalid
```

**Python:**
```python
strata.verify_image("release.st", "keys/public.key")  # raises on failure
# Or use the high-level verifier:
valid = strata.verify("release.st", public_key="keys/public.key")
if not valid:
    raise RuntimeError("Snapshot verification failed")
```

#### Enforcing verification when loading

To fail if a snapshot is unsigned or invalid when opening it:

- **CLI:** Run `strata sys verify --key keys/public.key snapshot.st` before any operation that uses the snapshot; use a script or wrapper that always verifies first.
- **Python:** Call `strata.verify(path, public_key="keys/public.key")` before `strata.open(path)`. If you need to require signing for all reads, wrap the open step:

  ```python
  if not strata.verify(path, public_key=pub_key):
      raise ValueError("Snapshot must be verified before use")
  with strata.open(path) as reader:
      ...
  ```

---

## Common Workflows

### Dataset Distribution Pipeline

```bash
# 1. Create base dataset
strata data pack \\
  --disk imagenet-train.img \\
  --output imagenet-v1.st \\
  --compression zstd \\
  --block-size 131072

# 2. Upload to S3
aws s3 cp imagenet-v1.st s3://ml-datasets/

# 3. Users download and train
# (No extraction needed - read directly from snapshot)

# 4. Create incremental update
strata data pack \\
  --disk imagenet-v2-delta.img \\
  --output imagenet-v2.st \\
  --cdc  # Deduplicates common data with v1

# 5. Distribute update
aws s3 cp imagenet-v2.st s3://ml-datasets/
```

### VM Development Workflow

```bash
# 1. Install base OS
strata vm install \\
  --iso ubuntu-22.04.iso \\
  --output ubuntu-base.st \\
  --disk-size 40G \\
  --vnc

# 2. Boot and customize
strata vm boot ubuntu-base.st --snapshot
# Install dependencies, configure, test

# 3. Create clean snapshot after customization
strata vm mount ubuntu-base.st /mnt --overlay custom.img
# ... make changes via mount ...
sudo umount /mnt

strata vm commit \\
  --base ubuntu-base.st \\
  --overlay custom.img \\
  --output ubuntu-dev.st

# 4. Development iterations
strata vm boot ubuntu-dev.st \\
  --ram 8G \\
  --cpus 4 \\
  --net \\
  --forward 2222:22

# 5. Create release snapshot
strata vm commit \\
  --base ubuntu-dev.st \\
  --overlay final-changes.img \\
  --output ubuntu-release-v1.0.st \\
  --encrypt
```

### Snapshot Versioning

```bash
# Version 1: Initial release
strata data pack --disk v1.img --output dataset-v1.st

# Version 2: Thin snapshot (only changes)
strata vm mount dataset-v1.st /mnt --overlay v2-delta.img
# ... modify data ...
sudo umount /mnt

strata vm commit \\
  --base dataset-v1.st \\
  --overlay v2-delta.img \\
  --output dataset-v2.st \\
  --thin

# Version 3: Full snapshot (consolidate)
strata vm mount dataset-v2.st /mnt --overlay v3-delta.img
# ... modify data ...
sudo umount /mnt

strata vm commit \\
  --base dataset-v2.st \\
  --overlay v3-delta.img \\
  --output dataset-v3.st
  # No --thin = creates standalone snapshot

# Storage:
#   dataset-v1.st: 10 GB (base)
#   dataset-v2.st: 500 MB (thin, depends on v1)
#   dataset-v3.st: 10.5 GB (standalone)
```

---

## Troubleshooting

### "Permission denied" on KVM

```
Error: Could not access KVM kernel module: Permission denied
```

**Fix**:
```bash
# Add user to kvm group
sudo usermod -a -G kvm $(whoami)

# Log out and back in, or:
newgrp kvm
```

### "FUSE: Device not found"

```
Error: /dev/fuse: No such file or directory
```

**Fix**:
```bash
# Load FUSE kernel module
sudo modprobe fuse

# Ensure FUSE is installed
sudo apt install fuse  # Debian/Ubuntu
sudo yum install fuse  # RHEL/CentOS
```

### Thin Snapshot "Parent not found"

```
Error: Parent snapshot not found: /old/path/base.st
```

**Cause**: Thin snapshot references base by absolute path, which has moved.

**Fix** (manual - future enhancement will automate):
```bash
# Option 1: Restore base to original location
mv base.st /old/path/

# Option 2: Convert thin to full snapshot
strata vm mount thin.st /mnt --overlay full.img
sudo umount /mnt
strata vm commit --base thin.st --overlay full.img --output standalone.st
# standalone.st is now independent
```

---

## Next Steps

- [Python Loader](../ai-loader/README.md) — Using snapshots in ML training
- [Internals](../../internals/format.md) — Understanding the snapshot format
- [Benchmarks](../../BENCHMARKS.md) — Performance characteristics
