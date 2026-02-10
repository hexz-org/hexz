# Strata Python Library

Python bindings for Strata, a snapshot storage system.

## Installation

This library requires a Rust compiler and is built from the **repository root** using the central Makefile.

**From source (recommended for development):** from repo root:

```bash
make develop
```

This installs the package in editable mode with the correct Rust + Python toolchain. To build a wheel only (e.g. for distribution): **`make python`**, then `pip install target/wheels/*.whl`.

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

Use the async context manager for asyncio integration.

```python
import asyncio
import strata

async def main():
    async with strata.AsyncReader("path/to/snapshot.st") as reader:
        data = await reader.read_at(0, 1024)

asyncio.run(main())
```

### Creating Snapshots

Create a new snapshot from files or in-memory data.

```python
import strata

# From a file
with strata.open("output.st", mode="w", compression="lz4") as w:
    w.add("source_disk.raw")

# Or use the Writer directly with add_file / add_bytes
with strata.Writer("output.st", compression="lz4") as w:
    w.add("source_disk.raw")
```

### Mounting

Mount a snapshot as a read-only filesystem (requires FUSE).

```python
import strata

with strata.mount("snapshot.st") as mp:
    print(f"Mounted at {mp.path}")
    # Access files in mp.path/disk
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

From repo root:

```bash
make test-python
```
