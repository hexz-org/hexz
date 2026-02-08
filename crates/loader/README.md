# Strata Python Library

Python bindings for Strata, a snapshot storage system.

## Installation

This library requires a Rust compiler to build.

```bash
pip install .
```

For development:

```bash
pip install maturin
maturin develop --release
```

## Usage

### Reading Snapshots

Read data from a Strata snapshot file.

```python
import strata

# Open a snapshot
reader = strata.open("path/to/snapshot.st")

# Read entire file
data = reader.read()

# Read specific range
chunk = reader.read_at(offset=1024, length=512)

# File-like interface
reader.seek(0)
header = reader.read(100)
```

### Async IO

Asynchronous support for integration with `asyncio`.

```python
import asyncio
import strata

async def main():
    reader = await strata.AsyncStrataReader.create("path/to/snapshot.st")
    data = await reader.read_at(0, 1024)

asyncio.run(main())
```

### Creating Snapshots

Create a new snapshot from a raw disk image.

```python
import strata

with strata.SnapshotBuilder("output.st", compression="lz4") as builder:
    builder.add_disk("source_disk.raw")
```

### Mounting

Mount a snapshot as a read-only filesystem (requires FUSE).

```python
import strata

with strata.mount("snapshot.st") as mount_point:
    print(f"Mounted at {mount_point}")
    # Access files in mount_point/disk
```

### NumPy Integration

Read data directly into NumPy arrays without extra copies.

```python
import strata
import numpy as np

reader = strata.open("data.st")
array = strata.read_array(reader, offset=0, shape=(100, 100), dtype=np.float32)
```

## Testing

Run the test suite using pytest:

```bash
pytest tests/
```
