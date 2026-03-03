# hexz-fuse

FUSE filesystem adapter for mounting Hexz archives as block device files.

## Overview

`hexz-fuse` provides a FUSE (Filesystem in Userspace) implementation that mounts Hexz archives as accessible filesystems. This enables standard tools (dd, qemu, parted, mount) to interact with compressed archives as if they were regular files or block devices.

The FUSE adapter is primarily used for **VM operations** where QEMU needs to access disk images stored in Hexz format.

## How It Works

When mounted, a Hexz archive appears as a minimal filesystem:

```
/mnt/archive/
├── disk       # Block device file (size = archive disk size)
└── memory     # Optional auxiliary stream (if present in archive)
```

Reads from these files transparently decompress blocks on-the-fly. Optional overlay support enables copy-on-write semantics.

## Features

- **Transparent Decompression**: Reads decompress blocks automatically
- **Overlay Support**: Writes go to separate overlay file (copy-on-write)
- **Standard Tools**: Compatible with dd, qemu, mount, parted, fdisk, etc.
- **Random Access**: Efficient seeking without full decompression
- **Read-Only or Copy-on-Write**: Mount immutable or with writable overlay
- **Low Latency**: ~80μs cached reads, ~1ms uncached

## Quick Example

### Command-Line Usage

```bash
# Mount a archive (requires hexz CLI with fuse feature)
hexz vm mount archive.hxz /mnt/archive

# Access the disk image
sudo dd if=/mnt/archive/disk of=output.raw bs=1M count=100

# Boot a VM using the mounted disk
qemu-system-x86_64 -drive file=/mnt/archive/disk,format=raw

# Unmount
fusermount -u /mnt/archive
```

### Programmatic Usage

```rust
use hexz_fuse::mount_fs;
use hexz_core::File;
use hexz_core::store::local::FileBackend;
use hexz_core::algo::compression::lz4::Lz4Compressor;
use std::sync::Arc;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    // Open archive
    let backend = Arc::new(FileBackend::new("archive.hxz".as_ref())?);
    let compressor = Box::new(Lz4Compressor::new());
    let snap = Arc::new(File::new(backend, compressor, None)?);

    // Mount at /mnt/archive with overlay
    mount_fs(
        snap,
        Path::new("/mnt/archive"),
        Some(Path::new("overlay.bin")), // Copy-on-write overlay
        1000,  // UID
        1000   // GID
    )?;

    Ok(())
}
```

## Architecture

```
hexz-fuse/
├── src/
│   ├── lib.rs          # Public API (mount_fs)
│   ├── fuse/           # FUSE filesystem implementation
│   │   ├── fs.rs       # FUSE trait implementation
│   │   └── ops.rs      # File operations (read, write, stat)
│   └── vfs/            # Virtual filesystem abstractions
│       ├── inode.rs    # Inode management
│       ├── attrs.rs    # File attributes
│       └── overlay.rs  # Copy-on-write overlay
```

## Copy-on-Write Overlay

When mounted with an overlay file, the FUSE filesystem provides copy-on-write semantics:

- **Reads**: Check overlay first; if not present, read from base archive
- **Writes**: Store in overlay file; base archive remains immutable
- **Commit**: Use `hexz vm commit` to merge overlay into a new archive

```bash
# Mount with overlay
hexz vm mount base.hxz /mnt/vm --overlay changes.bin

# Make modifications (e.g., install software in VM)
# All writes go to changes.bin

# Commit overlay to new archive
hexz vm commit --overlay changes.bin --base base.hxz --output updated.hxz
```

This is useful for:
- VM archiveting and rollback
- Testing changes without modifying originals
- Incremental backups

## Use Cases

### VM Boot

Boot a virtual machine from a Hexz archive:

```bash
# Mount archive
hexz vm mount ubuntu.hxz /mnt/ubuntu

# Boot with QEMU
qemu-system-x86_64 \
  -drive file=/mnt/ubuntu/disk,format=raw \
  -m 4G \
  -enable-kvm

# Or use the integrated boot command
hexz vm boot ubuntu.hxz --ram 4G
```

### Disk Image Manipulation

Use standard tools on compressed archives:

```bash
# Mount archive
hexz vm mount disk.hxz /mnt/disk

# Partition with parted
sudo parted /mnt/disk/disk print

# Copy data with dd
sudo dd if=/mnt/disk/disk of=backup.raw bs=1M

# Mount a partition (requires offset calculation)
sudo mount -o loop,offset=1048576 /mnt/disk/disk /mnt/partition
```

### Extract Files

Access individual files without full decompression:

```bash
# Mount archive
hexz vm mount archive.hxz /mnt/snap

# Mount the disk's filesystem (assuming ext4 at offset 0)
sudo mount -o loop /mnt/snap/disk /mnt/contents

# Copy specific files
cp /mnt/contents/path/to/file.txt ./
```

## Performance

| Metric | Value |
|--------|-------|
| Read Latency (cached) | ~80 μs |
| Read Latency (uncached) | ~1 ms |
| Write Latency (overlay) | ~50 μs |
| Sequential Throughput | ~2-3 GB/s (LZ4) |
| Random Read Throughput | ~500 MB/s |

Performance depends on:
- Compression algorithm (LZ4 faster than Zstd)
- Cache size (more cache = fewer decompression operations)
- Storage backend (local NVMe faster than S3)

## Requirements

### System Requirements

- **Linux**: FUSE support (kernel module loaded)
- **macOS**: macFUSE installed
- **User permissions**: Ability to mount filesystems (or use sudo)

### Installation

```bash
# Linux (Ubuntu/Debian)
sudo apt-get install fuse libfuse-dev

# Linux (Fedora/RHEL)
sudo dnf install fuse fuse-devel

# macOS
brew install macfuse
```

### Check FUSE Support

```bash
# Linux - check if FUSE module is loaded
lsmod | grep fuse

# Check fusermount is available
which fusermount
```

## Development

From the repository root:

```bash
# Build fuse crate
cargo build -p hexz-fuse

# Run tests
cargo test -p hexz-fuse

# Build CLI with FUSE support
make rust
```

### Testing

```bash
# Run FUSE-specific tests
cargo test -p hexz-fuse

# Integration tests (requires FUSE available)
cargo test -p hexz-fuse --test integration
```

## Limitations

- **Linux/macOS only**: FUSE not available on Windows (use WSL2)
- **Single-threaded**: Current implementation uses single-threaded FUSE
- **Read-mostly**: Write performance limited by overlay format
- **Permissions**: May require sudo/root for mounting

## See Also

- **[hexz-core](../core/)** - Core engine (provides File)
- **[hexz-cli](../cli/)** - CLI tool (mount/boot commands)
- **[User Documentation](../../docs/)** - VM usage guides
- **[Project README](../../README.md)** - Main project overview
