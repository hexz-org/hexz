# Strata Python Loader — AI/ML Dataset Management

The Strata Python loader provides high-performance, zero-copy access to compressed snapshot data, designed specifically for machine learning training pipelines. Unlike traditional dataset formats that require full decompression or extensive preprocessing, Strata enables random access to individual samples while maintaining efficient compression ratios.

## Why Strata for ML?

**Problem**: Modern ML datasets (ImageNet, COCO, custom medical imaging) are massive. Traditional approaches either:
- Store uncompressed data (hundreds of GB → TB)
- Use per-sample compression (tar.gz, zip) requiring full decompression
- Copy entire datasets to local NVMe before training

**Strata's Solution**:
- **Block-level compression**: Only decompress the blocks you need
- **Remote streaming**: Train directly from S3/HTTP without local copies
- **Zero-copy reads**: Direct memory mapping into NumPy/PyTorch tensors
- **Snapshot consistency**: Entire dataset versions with deduplication

## Quick Start

### Installation

```bash
# From PyPI (when published)
pip install strata

# From source
cd crates/loader
maturin develop --release
```

### Basic Usage

```python
import strata
import numpy as np

# Open a snapshot (local, S3, or HTTP)
reader = strata.StrataReader("dataset.st")

# Read raw bytes at offset
data = reader.read_at(offset=1024, length=4096)

# Zero-copy read into NumPy array
buffer = np.zeros(4096, dtype=np.uint8)
reader.read(buffer=buffer)  # Fills buffer without intermediate copy

# File-like interface
reader.seek(0)
chunk = reader.read(8192)
```

### Remote Streaming

```python
# Stream directly from S3 (no local download)
reader = strata.StrataReader(
    "s3://my-bucket/datasets/imagenet-train.st",
    s3_region="us-west-2"
)

# Or from HTTP/HTTPS
reader = strata.StrataReader(
    "https://datasets.example.com/coco-2017.st"
)
```

## PyTorch Integration

### Dataset Wrapper

```python
import torch
from torch.utils.data import Dataset, DataLoader
import strata
import numpy as np
from PIL import Image
import io

class StrataImageDataset(Dataset):
    """PyTorch dataset that reads images from Strata snapshots.

    Format: Each image is stored as [4-byte length][JPEG bytes]
    """
    def __init__(self, snapshot_path, transform=None):
        self.reader = strata.StrataReader(snapshot_path)
        self.transform = transform

        # Read index: [offset, length] pairs for each image
        # Stored in first 8*N bytes
        num_images_bytes = self.reader.read_at(0, 8)
        self.num_images = int.from_bytes(num_images_bytes, 'little')

        # Read all offsets at once (efficient for large datasets)
        index_size = self.num_images * 16  # 8 bytes offset + 8 bytes length
        index_bytes = self.reader.read_at(8, index_size)
        self.index = np.frombuffer(index_bytes, dtype=np.uint64).reshape(-1, 2)

    def __len__(self):
        return self.num_images

    def __getitem__(self, idx):
        offset, length = self.index[idx]

        # Read JPEG bytes for this image
        jpeg_bytes = self.reader.read_at(offset, length)

        # Decode image
        image = Image.open(io.BytesIO(jpeg_bytes))

        if self.transform:
            image = self.transform(image)

        return image

# Use with DataLoader
dataset = StrataImageDataset("s3://bucket/imagenet.st")
loader = DataLoader(
    dataset,
    batch_size=32,
    shuffle=True,
    num_workers=4,  # Parallel decompression across workers
    pin_memory=True
)

for batch in loader:
    # Train your model...
    pass
```

### Optimizations for Training

```python
# 1. Prefetch metadata during initialization
class OptimizedStrataDataset(Dataset):
    def __init__(self, snapshot_path, cache_index=True):
        self.reader = strata.StrataReader(snapshot_path)

        if cache_index:
            # Load entire index into memory (cheap for most datasets)
            self._cache_index()

    def _cache_index(self):
        # Implementation similar to above
        pass

# 2. Use memory mapping for local files
reader = strata.StrataReader(
    "/nvme/dataset.st",
    # Memory mapping happens automatically for local files
)

# 3. Batch reads for sequential access
offsets = [self.index[i][0] for i in range(batch_start, batch_end)]
lengths = [self.index[i][1] for i in range(batch_start, batch_end)]

# Read all images in batch with one call (reduces overhead)
batch_data = [
    self.reader.read_at(off, ln)
    for off, ln in zip(offsets, lengths)
]
```

## API Reference

### `StrataReader`

**Purpose**: Synchronous file-like interface for reading Strata snapshots with support for random access and zero-copy operations.

#### Constructor

```python
StrataReader(
    path: str,
    s3_region: Optional[str] = None,
    endpoint_url: Optional[str] = None,
    allow_restricted: bool = False
)
```

**Parameters**:
- `path`: Path or URI to snapshot. Supports:
  - Local paths: `/path/to/snapshot.st`
  - S3 URIs: `s3://bucket/key.st`
  - HTTP(S) URLs: `https://example.com/data.st`
- `s3_region`: AWS region for S3 access (default: `us-east-1`)
- `endpoint_url`: Custom S3 endpoint (for MinIO, Wasabi, etc.)
- `allow_restricted`: Allow connections to private IP ranges (security risk, disabled by default)

**Example**:
```python
# Local file
reader = StrataReader("/data/train.st")

# S3 with custom region
reader = StrataReader(
    "s3://ml-datasets/imagenet.st",
    s3_region="eu-west-1"
)

# MinIO/custom S3
reader = StrataReader(
    "s3://mybucket/data.st",
    endpoint_url="https://minio.example.com:9000"
)
```

#### Methods

##### `size() -> int`

Returns the total uncompressed size of the snapshot in bytes.

```python
total_bytes = reader.size()
print(f"Dataset size: {total_bytes / 1e9:.2f} GB")
```

##### `read_at(offset: int, length: int) -> bytes`

Reads exactly `length` bytes starting at `offset`. This is the most efficient method for random access as it only decompresses the necessary blocks.

**Performance characteristics**:
- Decompresses only the blocks containing the requested range
- Uses LRU cache for recently accessed blocks
- Thread-safe: multiple threads can call this concurrently

```python
# Read 1KB starting at 1MB offset
data = reader.read_at(1024 * 1024, 1024)

# Read image at known offset
image_bytes = reader.read_at(sample_offset, sample_size)
```

##### `read(size: Optional[int] = None) -> bytes`

Reads `size` bytes from current cursor position, or all remaining bytes if `size` is None. Advances the cursor.

```python
# Read next 4KB
chunk = reader.read(4096)

# Read all remaining data
remaining = reader.read()
```

##### `read(buffer: bytearray | np.ndarray) -> int`

**Zero-copy read** into a pre-allocated writable buffer. This is the most efficient method for repeated reads of the same size.

**Returns**: Number of bytes actually read (may be less than buffer size at EOF)

**Performance benefits**:
- No intermediate Python bytes object allocation
- Direct write into NumPy arrays or C buffers
- Ideal for hot loops in training

```python
import numpy as np

# Allocate reusable buffer
buffer = np.zeros(65536, dtype=np.uint8)

# Repeatedly read into same buffer (zero allocation)
for _ in range(1000):
    n = reader.read(buffer=buffer)
    if n == 0:
        break
    process_data(buffer[:n])
```

##### `seek(offset: int, whence: int = 0) -> int`

Moves the read cursor. Returns new absolute position.

**whence**:
- `0` (SEEK_SET): Absolute position
- `1` (SEEK_CUR): Relative to current position
- `2` (SEEK_END): Relative to end

```python
# Seek to start
reader.seek(0)

# Skip 1MB forward
reader.seek(1024 * 1024, 1)

# Seek to last 100 bytes
reader.seek(-100, 2)
```

##### `tell() -> int`

Returns current cursor position.

##### Context Manager Support

```python
with StrataReader("data.st") as reader:
    data = reader.read_at(0, 1024)
# Automatically closes reader
```

### `AsyncStrataReader`

**Purpose**: Asynchronous interface for asyncio-based applications. Useful for concurrent data loading in async frameworks.

#### Constructor

```python
# Must use async factory method
reader = await AsyncStrataReader.create(
    path="s3://bucket/data.st",
    s3_region="us-east-1"
)
```

#### Methods

All methods are async versions of `StrataReader`:

```python
async with await AsyncStrataReader.create("data.st") as reader:
    size = reader.size()  # Synchronous
    data = await reader.read_at(0, 1024)  # Async
    await reader.seek(1000)
```

**Use case**: Concurrent loading of multiple snapshots:

```python
import asyncio

async def load_batch(reader, offsets):
    tasks = [reader.read_at(off, size) for off in offsets]
    return await asyncio.gather(*tasks)

async def main():
    reader = await AsyncStrataReader.create("data.st")
    batches = await load_batch(reader, [0, 1000, 2000, 3000])
```

### `pack()`

**Purpose**: Create Strata snapshots from raw disk/memory images programmatically.

```python
strata.pack(
    output="snapshot.st",
    disk="/path/to/disk.img",          # Optional
    memory="/path/to/memory.dump",     # Optional
    compression="lz4",                 # "lz4" or "zstd"
    block_size=65536,                  # Block size in bytes
    encrypt=False,                     # Enable AES-GCM encryption
    password=None,                     # Required if encrypt=True
    cdc=False,                         # Content-defined chunking
    min_chunk=16384,                   # CDC: minimum chunk size
    avg_chunk=65536,                   # CDC: target average
    max_chunk=131072                   # CDC: maximum chunk size
)
```

**Compression comparison**:
- `lz4`: Fast compression/decompression (~2GB/s), moderate ratio (~2-3x)
- `zstd`: Better compression (~3-5x), slower (~500MB/s)

**When to use CDC (Content-Defined Chunking)**:
- Datasets with duplicated content (deduplication)
- Incremental snapshots (only store changed chunks)
- Trade-off: Slower packing, better compression for redundant data

```python
# Create snapshot from disk image
strata.pack(
    output="base.st",
    disk="ubuntu-base.img",
    compression="zstd"  # Better compression for base images
)

# Create encrypted snapshot
strata.pack(
    output="secure.st",
    disk="sensitive-data.img",
    encrypt=True,
    password="strong-passphrase"
)
```

## Advanced Usage

### Multi-Stream Snapshots

Strata supports separate disk and memory streams in a single snapshot (useful for VM snapshots):

```python
# Access disk stream (default)
reader = StrataReader("vm-snapshot.st")
disk_data = reader.read_at(0, 1024)

# Access memory stream (requires special API - future enhancement)
# memory_data = reader.read_memory_at(0, 1024)
```

### Error Handling

```python
from strata import StrataReader
import logging

try:
    reader = StrataReader("s3://bucket/missing.st")
except IOError as e:
    logging.error(f"Failed to open snapshot: {e}")
    # Handle: file not found, network error, auth failure

try:
    data = reader.read_at(0, 999999999999)
except IOError as e:
    logging.error(f"Read failed: {e}")
    # Handle: offset out of bounds, decompression error
```

### Performance Tuning

```python
# 1. Block size affects compression ratio vs. random access
strata.pack(
    output="data.st",
    disk="source.img",
    block_size=16384   # Smaller = better random access, worse compression
)

# 2. Use local caching for S3 datasets
# (Future enhancement: automatic local cache)

# 3. Prefetch for sequential access
# Read ahead in background thread (implemented by DataLoader num_workers)
```

## Troubleshooting

### Slow S3 Access

**Symptom**: Training slower than expected with S3 snapshots

**Solutions**:
1. Use `DataLoader` with `num_workers > 0` for parallel prefetching
2. Increase `block_size` during pack to reduce metadata overhead
3. Ensure your instance has sufficient network bandwidth to S3
4. Consider regional data transfer costs and co-locate compute with storage

### Memory Usage

**Symptom**: High memory consumption during training

**Cause**: Default block cache size

**Solution**: The internal LRU cache is bounded. For very large datasets with random access:
- Use smaller `block_size` when creating snapshots
- Use multiple smaller snapshots instead of one huge file
- Implement custom sampling strategy to improve cache locality

### Decompression Overhead

**Symptom**: CPU bottleneck during data loading

**Solutions**:
1. Switch to `lz4` compression (faster decompression)
2. Increase `DataLoader` workers to parallelize decompression
3. Use local NVMe cache for hot datasets
4. Consider uncompressed regions for frequently accessed data (future enhancement)

## Migration Guide

### From TFRecord

```python
# Before (TFRecord)
import tensorflow as tf

dataset = tf.data.TFRecordDataset("data.tfrecord")
dataset = dataset.map(parse_function)

# After (Strata)
from torch.utils.data import DataLoader
dataset = StrataImageDataset("data.st")
loader = DataLoader(dataset, batch_size=32, num_workers=4)
```

### From HDF5

```python
# Before (HDF5)
import h5py

with h5py.File("data.h5", "r") as f:
    data = f["images"][idx]

# After (Strata)
reader = StrataReader("data.st")
data = reader.read_at(offset_for_idx, length_for_idx)
```

**Advantages over HDF5**:
- Better compression
- S3/HTTP streaming without local copy
- Simpler format (no schema overhead)
- Thread-safe concurrent access

## Next Steps

- [CLI Guide](../cli/README.md) — Creating snapshots with the `strata` command
- [Internals](../../internals/format.md) — Understanding the snapshot format
- [Benchmarks](../../BENCHMARKS.md) — Performance characteristics
