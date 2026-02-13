# strata-loader

Python bindings for Strata - high-performance ML data loading with zero-copy reads and background prefetching.

## Overview

`strata-loader` provides Python bindings to the Strata engine via PyO3. It's designed for **AI/ML training workflows** where you need to stream massive datasets directly from compressed storage (local files, S3, HTTP) into GPU memory without Python GIL overhead.

The loader bypasses Python's multiprocessing by handling prefetching in lightweight Rust threads, eliminating "GPU starvation" during training.

## Installation

### From PyPI (Coming Soon)

```bash
# Minimal installation (core features only, ~5MB)
pip install strata

# With PyTorch support
pip install strata[torch]

# With TensorFlow support
pip install strata[tensorflow]

# With NumPy arrays
pip install strata[numpy]

# ML bundle (PyTorch + NumPy)
pip install strata[ml]

# Everything
pip install strata[full]

# Development
pip install strata[dev]
```

### From Source (Development)

Build and install from the repository root using the Makefile:

```bash
# One-time setup (creates venv, installs tools)
make setup

# Install in editable mode (recommended for development)
make develop

# Or build a wheel for distribution
make python
pip install target/wheels/*.whl
```

**Note**: Requires Rust toolchain and Python 3.8+. Run `make setup-check` to verify dependencies.

### Custom Feature Selection

For advanced users who want to control binary size and compile-time features:

```bash
# Build with minimal features (no S3, zstd compression only)
maturin build --release --no-default-features --features compression-zstd

# Build with S3 but no compression-zstd (LZ4 only)
maturin build --release --no-default-features --features s3

# Build with all features
maturin build --release --features full

# Install custom build
pip install target/wheels/*.whl
```

**Binary Size Comparison (release, stripped):**
- Minimal build (no default features): 12MB
- Default (S3 + zstd + signing): 12MB
- Full features: 12MB

**Note**: Binary size is dominated by PyO3 runtime and Tokio async runtime. The main benefits of feature gates are:
- Reduced dependency complexity and faster compilation
- Smaller dependency tree (fewer crates to audit/update)
- Cleaner runtime without unused functionality

## Quick Start

### PyTorch Integration

Drop-in replacement for standard PyTorch datasets:

```python
import torch
from strata import StrataLoader

# Open a compressed dataset (local or remote)
dataset = StrataLoader("s3://my-bucket/imagenet.st")

# Standard PyTorch DataLoader
# Strata handles prefetching in Rust background threads
loader = torch.utils.data.DataLoader(
    dataset,
    batch_size=64,
    num_workers=4
)

for batch in loader:
    # GPU is fed instantly with zero-copy overhead
    train_step(batch)
```

### Reading Snapshots

Simple file-like interface for reading Strata files:

```python
import strata

# Open a snapshot
reader = strata.open("path/to/snapshot.st")

# Read entire file
data = reader.read()

# Read specific range
chunk = reader.read_at(offset=1024, length=512)

# File-like seek/read
reader.seek(0)
header = reader.read(100)
```

### Async I/O

Async context manager for asyncio integration:

```python
import asyncio
import strata

async def main():
    async with strata.AsyncReader("path/to/snapshot.st") as reader:
        data = await reader.read_at(0, 1024)

asyncio.run(main())
```

## Key Features

- **Zero-Copy Reads**: Direct memory access without Python overhead
- **Background Prefetching**: Rust threads handle I/O while Python/GPU computes
- **PyTorch Integration**: `StrataDataset` implements PyTorch's Dataset interface
- **Remote Streaming**: Stream from S3/HTTP without downloading entire files
- **NumPy Integration**: Read directly into NumPy arrays
- **Encryption Support**: Transparent decryption of encrypted snapshots
- **GIL-Free**: Critical paths run in Rust without Python GIL contention

## Feature Matrix

Strata is designed with modularity in mind. Install only what you need:

| Feature | Default | Description | Size Impact |
|---------|---------|-------------|-------------|
| **LZ4 Compression** | ✅ | Fast compression (always included) | ~1MB |
| **S3 Storage** | ✅ | Stream from AWS S3, MinIO, Cloudflare R2 | ~3MB |
| **Zstd Compression** | ✅ | High-ratio compression | ~2MB |
| **Encryption** | ❌ | AES-GCM encryption for snapshots | ~1MB |
| **Signing** | ❌ | Ed25519 cryptographic signatures | ~500KB |

### Python Extras

| Extra | Includes | Use Case |
|-------|----------|----------|
| `[torch]` | PyTorch ≥2.0 | ML training with PyTorch DataLoader |
| `[tensorflow]` | TensorFlow ≥2.13 | ML training with TensorFlow Dataset |
| `[numpy]` | NumPy ≥1.20 | Scientific computing, array operations |
| `[ml]` | NumPy + PyTorch | Common ML stack |
| `[full]` | All ML frameworks | Everything for ML workflows |
| `[dev]` | Testing + linting tools | Development and contribution |

### Compile-Time Features

Control Rust features at build time for minimal deployments:

```bash
# Minimal: local files only, LZ4 compression
maturin build --no-default-features

# Add S3 support
maturin build --no-default-features --features s3

# Add encryption
maturin build --no-default-features --features encryption,s3

# Everything
maturin build --features full
```

**Use Cases:**
- **Edge Deployments**: Disable S3 to reduce binary size for IoT/embedded
- **Air-Gapped Systems**: Build without network features for secure environments
- **Size-Constrained Containers**: Minimal builds for Lambda/Cloud Run

## Architecture

```
strata-loader/
├── src/                    # Rust source (PyO3 bindings)
│   ├── lib.rs             # Main Python module
│   ├── reader.rs          # Reader bindings
│   ├── writer.rs          # Writer bindings
│   └── utils.rs           # Helper functions
├── python/strata/         # Python wrapper code
│   ├── __init__.py        # Public API
│   ├── dataset.py         # PyTorch Dataset integration
│   ├── reader.py          # High-level reader interface
│   ├── writer.py          # High-level writer interface
│   ├── array.py           # NumPy integration
│   ├── torch/             # PyTorch utilities
│   └── ml/                # ML-specific helpers
├── tests/                 # Python tests (pytest)
└── examples/              # Usage examples
```

## Usage Examples

### Creating Snapshots

Create snapshots from Python:

```python
import strata

# From a file
with strata.open("output.st", mode="w", compression="lz4") as w:
    w.add("source_disk.raw")

# Or use Writer directly
with strata.Writer("output.st", compression="lz4") as w:
    w.add_file("source_disk.raw")
    w.add_bytes(b"additional data")
```

### NumPy Integration

Read data directly into NumPy arrays without extra copies:

```python
import strata
import numpy as np

reader = strata.open("data.st")

# Zero-copy read into NumPy array
array = strata.read_array(
    reader,
    offset=0,
    shape=(100, 100),
    dtype=np.float32
)
```

### Mounting Snapshots

Mount as a read-only filesystem (requires FUSE):

```python
import strata

with strata.mount("snapshot.st") as mp:
    print(f"Mounted at {mp.path}")
    # Access files in mp.path/disk
```

### Remote Streaming

Stream from S3 or HTTP:

```python
import strata

# S3 streaming
dataset = strata.open("s3://bucket/dataset.st")

# HTTP streaming
dataset = strata.open("https://example.com/data.st")

# Read on-demand (only fetches needed blocks)
chunk = dataset.read_at(1024 * 1024, 4096)
```

## Development

All development commands use the project Makefile from the repository root.

### Building

```bash
# Install in editable mode (development)
make develop

# Build wheel for distribution
make python

# Build with specific Python version
PYTHON=python3.11 make develop
```

### Testing

```bash
# Run all tests (Rust + Python)
make test

# Run only Python tests
make test-python

# Run with filter
make test-python test_reader

# Or use pytest directly
pytest crates/loader/tests/ -v
```

### Linting & Formatting

```bash
# Format all code (Rust + Python)
make fmt

# Lint (includes ruff for Python)
make lint

# Python-specific linting
ruff check crates/loader/python/
```

See `make help` for all available commands.

## API Reference

### Core Types

- **`Reader`**: Read snapshots with file-like interface
- **`AsyncReader`**: Async I/O reader
- **`Writer`**: Create new snapshots
- **`StrataDataset`**: PyTorch Dataset implementation
- **`StrataLoader`**: High-level loader (alias for `StrataDataset`)

### Functions

- **`open(path, mode='r', **kwargs)`**: Open a snapshot (reader or writer)
- **`read_array(reader, offset, shape, dtype)`**: Zero-copy read into NumPy
- **`mount(path)`**: Mount snapshot as FUSE filesystem

See the [Python API documentation](../../docs/reference/python-api.md) for complete reference.

## Performance

Optimized for ML training workloads:

| Metric | Value |
|--------|-------|
| Sequential Read | ~2-3 GB/s |
| Random Access | ~1ms (cold), ~0.08ms (warm) |
| Prefetch Threads | Configurable (default: 4) |
| Memory Overhead | <150 MB per reader |
| Zero-Copy | Yes (via PyO3 buffer protocol) |

## PyTorch Integration

The `StrataDataset` class implements PyTorch's `Dataset` interface:

```python
from strata import StrataDataset
from torch.utils.data import DataLoader

# Create dataset
dataset = StrataDataset(
    "s3://bucket/train.st",
    transform=None,  # Optional transform function
    cache_size=1024  # Cache 1024 blocks in memory
)

# Use with DataLoader
loader = DataLoader(
    dataset,
    batch_size=32,
    num_workers=4,
    shuffle=True
)
```

## Requirements

- **Python**: 3.8+ (ABI3 compatible)
- **Rust**: Latest stable (for building from source)
- **System**: Linux, macOS, or Windows
- **Optional**: FUSE (for mounting)

## See Also

- **[User Documentation](../../docs/)** - Tutorials and guides
- **[Python API Reference](../../docs/reference/python-api.md)** - Complete API docs
- **[strata-core](../core/)** - Core Rust engine
- **[CLI Tool](../cli/)** - Command-line interface
- **[Project README](../../README.md)** - Main project overview
