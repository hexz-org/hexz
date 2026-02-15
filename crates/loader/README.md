# hexz

[![PyPI](https://img.shields.io/pypi/v/hexz)](https://pypi.org/project/hexz/)
[![Python](https://img.shields.io/pypi/pyversions/hexz)](https://pypi.org/project/hexz/)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](https://github.com/hexz-org/hexz/blob/main/LICENSE)

Python library for reading, writing, and streaming [Hexz](https://github.com/hexz-org/hexz) snapshots — a seekable, deduplicated compression format built in Rust.

```bash
pip install hexz
```

## Quick Start

### Reading snapshots

```python
import hexz

with hexz.open("data.hxz") as reader:
    data = reader.read()           # read entire snapshot
    chunk = reader.read(4096)      # read 4KB from current position
    reader.seek(1024)              # seek to offset
    block = reader[100:200]        # slice notation
```

### Writing snapshots

```python
import hexz

with hexz.open("output.hxz", mode="w", compression="lz4") as writer:
    writer.add_file("disk.img")
    writer.add_bytes(b"extra data")
```

### Building from files

```python
import hexz

# Build with a profile preset (ml, eda, embedded, generic, archival)
metadata = hexz.build("source.img", "output.hxz", profile="ml")
```

### Converting from other formats

```python
import hexz

# Convert tar, HDF5, or WebDataset archives
hexz.convert("dataset.tar", "dataset.hxz")
hexz.convert("data.h5", "data.hxz")       # requires pip install hexz[hdf5]
```

### Remote storage

```python
import hexz

# Stream from S3 (only fetches needed blocks)
reader = hexz.open("s3://bucket/data.hxz", s3_region="us-east-1")
chunk = reader.read(4096)

# HTTP streaming
reader = hexz.open("https://example.com/data.hxz")
```

## API

### Core I/O

| Function / Class | Description |
|---|---|
| `hexz.open(path, mode="r", **opts)` | Open a snapshot for reading or writing |
| `Reader(path, ...)` | Read snapshots with file-like interface (seek, read, tell, slice) |
| `AsyncReader.create(path, ...)` | Async reader for asyncio workflows |
| `Writer(path, ...)` | Create new snapshots with compression and deduplication |

### Data operations

| Function | Description |
|---|---|
| `build(source, output, profile, ...)` | Build snapshot from files with preset profiles |
| `convert(input, output, format, ...)` | Convert tar/HDF5/WebDataset to Hexz |
| `inspect(path)` | Get snapshot metadata (compression, size, block count) |
| `verify(path, ...)` | Verify integrity and optional cryptographic signature |

### Array support

| Function / Class | Description |
|---|---|
| `read_array(source, offset, shape, dtype)` | Read NumPy array from snapshot |
| `write_array(dest, array, ...)` | Write NumPy array to snapshot |
| `ArrayView(path, shape, dtype)` | Memory-mapped array access with slicing |

### ML integration

| Class | Description |
|---|---|
| `Dataset(path, ...)` | PyTorch `Dataset` with caching and prefetching |

```python
from hexz import Dataset
from torch.utils.data import DataLoader

dataset = Dataset("s3://bucket/train.hxz", cache_size_mb=512)
loader = DataLoader(dataset, batch_size=32, num_workers=4, shuffle=True)

for batch in loader:
    train_step(batch)
```

### Cryptographic operations

```python
import hexz

hexz.keygen("private.key", "public.key")   # generate Ed25519 keypair
hexz.sign("snapshot.hxz", "private.key")   # sign a snapshot
hexz.verify("snapshot.hxz", "public.key")  # verify signature
```

## Optional dependencies

```bash
pip install hexz[numpy]       # NumPy array support
pip install hexz[torch]       # PyTorch Dataset integration
pip install hexz[hdf5]        # HDF5 conversion (h5py)
pip install hexz[ml]          # NumPy + PyTorch
pip install hexz[full]        # All optional dependencies
```

## Compression

LZ4 is always available. Zstd and S3 streaming are included by default in PyPI wheels.

| Algorithm | Speed | Ratio | Default |
|---|---|---|---|
| LZ4 | Fast (~2-3 GB/s) | Moderate | Always included |
| Zstd | Moderate | High | Yes |

## Requirements

- Python 3.8+
- Linux (x86_64, aarch64), macOS (x86_64, Apple Silicon), or Windows (x86_64)

## Links

- [GitHub](https://github.com/hexz-org/hexz)
- [Documentation](https://hexz-org.github.io/hexz/)
- [CLI Tool](https://github.com/hexz-org/hexz/releases) — `hexz` command-line binary
