# Hexz

High-performance, deduplicated archive format for large-scale data distribution.

Hexz is a single-file archive format (`.hxz`) designed for storing and distributing massive datasets, disk images, and binary blobs. Unlike traditional archive formats (tar, zip) or deduplication tools (Git LFS), Hexz archives are **natively seekable**, allowing you to mount a multi-terabyte archive and access any byte instantly without decompressing the entire file.

## Key Features

- **Block-Level Deduplication:** Uses Content-Defined Chunking (CDC) to identify shared blocks across different versions of the same data, even if offsets shift.
- **Random Access (Seekable):** O(1) seek time. Mount archives via FUSE to treat them as standard block devices or filesystems.
- **Thin Archives (Deltas):** Create archives that only store changed blocks, referencing a "base" archive for common data. Ideal for versioning large datasets.
- **Git-like Workspaces:** Checkout archives into writable workspaces. Hexz creates a standard, transparent directory structure using advanced FUSE passthrough while keeping state securely isolated in global storage (`~/.hexz/workspaces/`).
- **Transparent Compression & Encryption:** Supports LZ4 and Zstandard compression, and AES-256-GCM encryption.
- **Self-Contained:** A single `.hxz` file contains the data, the index, and the metadata. No complex repository structure required.
- **Cloud Native:** Supports byte-range requests for on-demand fetching from S3-compatible object storage.

## Why Hexz?

| Feature | Git LFS | Hexz |
| :--- | :--- | :--- |
| **Deduplication** | File-level | **Block-level (CDC)** |
| **Access Pattern** | Full Download | **Random Access (Seekable)** |
| **Update Efficiency** | Re-upload whole file | **Only upload changed blocks** |
| **Mountable** | No | **Yes (FUSE)** |

## Quick Start

### Install
```bash
make install
```

### Pack an archive
```bash
hexz pack ./large_dataset.bin data.hxz --compression zstd
```

### Mount and inspect
```bash
mkdir /mnt/data
hexz mount data.hxz /mnt/data
ls -lh /mnt/data
# Access contents via /mnt/data/disk
```

### Create a thin delta
```bash
# Create a new version that dedups against the base
hexz pack ./large_dataset_v2.bin v2.hxz --base v1.hxz
```

### Workspaces (Git-like Workflow)
```bash
# Checkout an archive into a writable workspace
hexz checkout data.hxz ./my_workspace
cd my_workspace

# Make changes to the mounted files
echo "New data" > README.md

# View changes
hexz status

# Commit changes to a new thin archive (delta)
hexz commit ../data_v2.hxz
```

## Documentation

See the `docs/` directory for detailed information on:
- [Architecture](docs/explanation/architecture.md)
- [CDC & Deduplication](docs/explanation/cdc.md)
- [FUSE & NBD Mounting](docs/reference/mount.md)

## License

Licensed under [Apache 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT).
