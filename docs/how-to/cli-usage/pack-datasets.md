# Pack Datasets with the CLI

**Goal**: Convert directories, disk images, or files into compressed Hexz snapshots using the command-line tool.

## Prerequisites

- Hexz CLI installed (`make rust`)
- Source data ready (directory, disk image, or files)

## Basic Usage

### Pack a Single File

```bash
hexz data pack \\
  --disk /path/to/file.img \\
  --output snapshot.hxz \\
  --compression lz4
```

### Pack a Directory

```bash
hexz data pack \\
  --disk /path/to/dataset/ \\
  --output dataset.hxz \\
  --compression zstd \\
  --block-size 131072
```

## Options

### Compression Algorithms

**LZ4** (fast decompression):
```bash
hexz data pack --disk data/ --output out.hxz --compression lz4
```
- Speed: ~2GB/s decompression
- Ratio: 2-3×
- Use for: Hot data, frequent access

**Zstandard** (better compression):
```bash
hexz data pack --disk data/ --output out.hxz --compression zstd --compression-level 9
```
- Speed: ~500MB/s decompression
- Ratio: 3-5×
- Use for: Cold storage, archival

### Block Size Tuning

```bash
# Small blocks (faster random access, less compression)
hexz data pack --disk data/ --output out.hxz --block-size 16384   # 16KB

# Default (balanced)
hexz data pack --disk data/ --output out.hxz --block-size 65536   # 64KB

# Large blocks (better compression, slower random access)
hexz data pack --disk data/ --output out.hxz --block-size 262144  # 256KB
```

### Enable Deduplication

```bash
hexz data pack \\
  --disk data/ \\
  --output dataset.hxz \\
  --cdc  # Content-Defined Chunking
```

Use `--cdc` when:
- Data has redundancy (duplicate files, repeated patterns)
- Creating incremental snapshots
- Storage cost is critical

## Advanced Features

### Encryption

```bash
# Generate key
hexz sys keygen --output-dir ./keys

# Pack with encryption
hexz data pack \\
  --disk sensitive-data/ \\
  --output encrypted.hxz \\
  --encrypt \\
  --key ./keys/public.key
```

### Signing

```bash
# Pack and sign
hexz data pack --disk data/ --output signed.hxz
hexz sys sign --key ./keys/private.key signed.hxz

# Verify
hexz sys verify --key ./keys/public.key signed.hxz
```

### Parent Snapshots (Incremental)

```bash
# Create base snapshot
hexz data pack --disk v1/ --output dataset-v1.hxz --cdc

# Create incremental update (references parent)
hexz data pack \\
  --disk v2/ \\
  --output dataset-v2.hxz \\
  --parent dataset-v1.hxz \\
  --cdc
```

## View Snapshot Info

```bash
hexz data info snapshot.hxz
```

Output:
```
Snapshot: snapshot.hxz
Format Version: 1
Compression: LZ4
Block Size: 65536
Uncompressed Size: 1.2 GB
Compressed Size: 456 MB
Compression Ratio: 2.63×
Block Count: 18750
Deduplication: Enabled
```

## See Also

- [Reference: CLI Commands](../../reference/cli-reference.md)
- [How-To: Install Hexz](install-hexz.md)
- [Tutorial: Getting Started](../../tutorials/getting-started.md)
