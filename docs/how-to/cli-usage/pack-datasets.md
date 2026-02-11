# Pack Datasets with the CLI

**Goal**: Convert directories, disk images, or files into compressed Strata snapshots using the command-line tool.

## Prerequisites

- Strata CLI installed (`make rust`)
- Source data ready (directory, disk image, or files)

## Basic Usage

### Pack a Single File

```bash
strata data pack \\
  --disk /path/to/file.img \\
  --output snapshot.st \\
  --compression lz4
```

### Pack a Directory

```bash
strata data pack \\
  --disk /path/to/dataset/ \\
  --output dataset.st \\
  --compression zstd \\
  --block-size 131072
```

## Options

### Compression Algorithms

**LZ4** (fast decompression):
```bash
strata data pack --disk data/ --output out.st --compression lz4
```
- Speed: ~2GB/s decompression
- Ratio: 2-3×
- Use for: Hot data, frequent access

**Zstandard** (better compression):
```bash
strata data pack --disk data/ --output out.st --compression zstd --compression-level 9
```
- Speed: ~500MB/s decompression
- Ratio: 3-5×
- Use for: Cold storage, archival

### Block Size Tuning

```bash
# Small blocks (faster random access, less compression)
strata data pack --disk data/ --output out.st --block-size 16384   # 16KB

# Default (balanced)
strata data pack --disk data/ --output out.st --block-size 65536   # 64KB

# Large blocks (better compression, slower random access)
strata data pack --disk data/ --output out.st --block-size 262144  # 256KB
```

### Enable Deduplication

```bash
strata data pack \\
  --disk data/ \\
  --output dataset.st \\
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
strata sys keygen --output-dir ./keys

# Pack with encryption
strata data pack \\
  --disk sensitive-data/ \\
  --output encrypted.st \\
  --encrypt \\
  --key ./keys/public.key
```

### Signing

```bash
# Pack and sign
strata data pack --disk data/ --output signed.st
strata sys sign --key ./keys/private.key signed.st

# Verify
strata sys verify --key ./keys/public.key signed.st
```

### Parent Snapshots (Incremental)

```bash
# Create base snapshot
strata data pack --disk v1/ --output dataset-v1.st --cdc

# Create incremental update (references parent)
strata data pack \\
  --disk v2/ \\
  --output dataset-v2.st \\
  --parent dataset-v1.st \\
  --cdc
```

## View Snapshot Info

```bash
strata data info snapshot.st
```

Output:
```
Snapshot: snapshot.st
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
- [How-To: Install Strata](install-strata.md)
- [Tutorial: Getting Started](../../tutorials/getting-started.md)
