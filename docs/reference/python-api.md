# Python API Reference

Complete reference for the Strata Python package.

## Installation

```bash
pip install strata
```

Or build from source:
```bash
git clone https://github.com/Alethic-Systems/strata.git
cd strata
make develop
```

## Module: `strata`

### Functions

#### `strata.open(path, mode='r', **kwargs)`

Open a Strata snapshot for reading or writing.

**Parameters**:
- `path` (str): Path to snapshot file. Supports:
  - Local files: `/path/to/file.st` or `file:///path/to/file.st`
  - S3: `s3://bucket/key.st`
  - HTTP(S): `https://example.com/dataset.st`
- `mode` (str): Open mode
  - `'r'`: Read (default)
  - `'w'`: Write (create new snapshot)
- `**kwargs`: Additional options

**Keyword Arguments (Read Mode)**:
- `s3_region` (str): AWS region for S3 URLs (default: auto-detect)
- `cache_size` (int): Block cache size in bytes (default: 256MB)
- `cache_dir` (str): Directory for disk cache (default: memory only)
- `retry_attempts` (int): Number of retries for remote reads (default: 3)
- `connect_timeout` (float): Connection timeout in seconds (default: 10)
- `read_timeout` (float): Read timeout in seconds (default: 30)

**Keyword Arguments (Write Mode)**:
- `compression` (str): Compression algorithm
  - `'lz4'`: Fast decompression (default)
  - `'zstd'`: Better compression ratio
- `compression_level` (int): Compression level (1-22 for zstd, ignored for lz4)
- `block_size` (int): Block size in bytes (default: 65536)
- `cdc` (bool): Enable content-defined chunking (default: False)
- `encrypt` (bool): Enable AES-256-GCM encryption (default: False)
- `encryption_key` (bytes): 32-byte encryption key (required if `encrypt=True`)

**Returns**: `StrataReader` (read mode) or `StrataWriter` (write mode)

**Raises**:
- `FileNotFoundError`: Snapshot not found (read mode)
- `PermissionError`: Insufficient permissions
- `ValueError`: Invalid parameters

**Examples**:

```python
# Read local snapshot
with strata.open("/data/dataset.st") as reader:
    data = reader.read(4096)

# Read from S3
with strata.open("s3://bucket/dataset.st", s3_region="us-west-2") as reader:
    data = reader.read(4096)

# Write new snapshot
with strata.open("output.st", mode="w", compression="zstd") as writer:
    writer.add("/data/images/")
```

---

### Class: `StrataReader`

Read-only access to Strata snapshots. Obtained via `strata.open(path, mode='r')`.

#### Methods

##### `read(size, offset=None, buffer=None)`

Read bytes from the snapshot.

**Parameters**:
- `size` (int): Number of bytes to read
- `offset` (int, optional): Absolute offset to read from. If None, reads from current position.
- `buffer` (buffer-like, optional): Pre-allocated buffer to read into (zero-copy). Must support buffer protocol (e.g., `numpy.ndarray`, `bytearray`).

**Returns**: `bytes` (if `buffer` is None) or `int` (bytes written to buffer)

**Example**:
```python
# Simple read
data = reader.read(4096)

# Read at offset
data = reader.read(1024, offset=1000000)

# Zero-copy into NumPy array
import numpy as np
buffer = np.zeros(4096, dtype=np.uint8)
reader.read(4096, buffer=buffer)
```

##### `seek(offset, whence=0)`

Move read position.

**Parameters**:
- `offset` (int): Offset in bytes
- `whence` (int): Reference point
  - `0` (SEEK_SET): Absolute position
  - `1` (SEEK_CUR): Relative to current position
  - `2` (SEEK_END): Relative to end

**Returns**: `int` - New absolute position

##### `tell()`

Get current read position.

**Returns**: `int` - Current position in bytes

##### `size()`

Get total uncompressed size of snapshot.

**Returns**: `int` - Size in bytes

##### `close()`

Close the snapshot and release resources. Called automatically when using `with` statement.

---

### Class: `StrataWriter`

Write data to a new Strata snapshot. Obtained via `strata.open(path, mode='w')`.

#### Methods

##### `add(path)`

Add a file or directory to the snapshot.

**Parameters**:
- `path` (str): Path to file or directory. Directories are added recursively.

**Example**:
```python
with strata.open("output.st", mode="w") as writer:
    writer.add("/data/image1.jpg")
    writer.add("/data/images/")  # Entire directory
```

##### `write(data)`

Write raw bytes to the snapshot.

**Parameters**:
- `data` (bytes or buffer-like): Data to write

**Returns**: `int` - Number of bytes written

**Example**:
```python
with strata.open("output.st", mode="w") as writer:
    writer.write(b"Hello, World!")
```

##### `close()`

Finalize the snapshot (writes index and header). Called automatically with `with` statement.

---

### Utility Functions

#### `strata.build(input_dir, output_path, **kwargs)`

High-level function to pack a directory into a snapshot.

**Parameters**:
- `input_dir` (str): Input directory path
- `output_path` (str): Output snapshot path
- `**kwargs`: Same as `strata.open(..., mode='w')`

**Example**:
```python
strata.build(
    "/data/imagenet",
    "imagenet.st",
    compression="zstd",
    compression_level=9,
    cdc=True
)
```

#### `strata.info(snapshot_path)`

Get snapshot metadata.

**Parameters**:
- `snapshot_path` (str): Path to snapshot

**Returns**: `dict` with keys:
- `format_version` (int)
- `compression` (str)
- `block_size` (int)
- `uncompressed_size` (int)
- `compressed_size` (int)
- `block_count` (int)
- `cdc_enabled` (bool)
- `encrypted` (bool)

**Example**:
```python
info = strata.info("dataset.st")
print(f"Compression ratio: {info['uncompressed_size'] / info['compressed_size']:.2f}×")
```

---

## PyTorch Integration

### Dataset Wrapper Example

```python
import torch
from torch.utils.data import Dataset
import strata

class StrataDataset(Dataset):
    def __init__(self, snapshot_path, item_size, transform=None):
        self.reader = strata.open(snapshot_path)
        self.item_size = item_size
        self.transform = transform
        self.length = self.reader.size() // item_size

    def __len__(self):
        return self.length

    def __getitem__(self, idx):
        offset = idx * self.item_size
        data = self.reader.read(self.item_size, offset=offset)

        if self.transform:
            data = self.transform(data)

        return data
```

---

## Exception Hierarchy

```
StrataError (base exception)
├── IoError - I/O failures
├── CorruptionError - Data integrity issues
├── FormatError - Invalid snapshot format
├── CompressionError - Compression/decompression failures
└── NetworkError - Remote backend failures
    ├── S3Error
    └── HttpError
```

---

## Type Stubs

Type hints are available for IDE autocomplete. Install with:
```bash
pip install strata[stubs]
```

---

## Thread Safety

- `StrataReader`: Thread-safe for concurrent reads
- `StrataWriter`: Not thread-safe, use from single thread
- GIL is released during I/O operations for true parallelism

---

## Version Compatibility

| Strata Version | Python Version | PyTorch Version |
|----------------|----------------|-----------------|
| 0.1.x          | 3.8+           | 1.12+           |

---

## See Also

- [Tutorial: First ML Pipeline](../tutorials/first-ml-pipeline.md)
- [How-To: Optimize PyTorch DataLoader](../how-to/ml-workflows/optimize-pytorch-dataloader.md)
- [API Source Code](https://github.com/Alethic-Systems/strata/tree/main/crates/loader)
