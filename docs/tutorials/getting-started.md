# Getting Started with Hexz

**Time:** ~10 minutes
**What you'll build:** A compressed archive, then read back from it with random access.

---

## Prerequisites

- Linux or macOS (Windows: core I/O works but not all features are validated)
- Python 3.8+
- Rust toolchain (for building from source)

---

## Install

**From PyPI (prebuilt):**
```bash
pip install hexz
```

**From source:**
```bash
git clone https://github.com/hexz-org/hexz.git
cd hexz
make develop   # builds Rust core and installs Python bindings
```

Verify:
```bash
python -c "import hexz; print(hexz.__version__)"
```

---

## Create and read an archive

```python
import hexz
import os

# 1. Write some data
with open("/tmp/hello.bin", "wb") as f:
    f.write(b"Hello, Hexz! " * 64)  # 960 bytes, repetitive

# 2. Pack into a Hexz archive
with hexz.open("/tmp/hello.hxz", mode="w", compression="lz4") as writer:
    writer.add_file("/tmp/hello.bin")

print(f"Original: 960 bytes")
print(f"Compressed: {os.path.getsize('/tmp/hello.hxz')} bytes")

# 3. Read back with random access
with hexz.open("/tmp/hello.hxz") as reader:
    data = reader.read(64)            # first 64 bytes
    reader.seek(100)
    data_at_100 = reader.read(30)     # 30 bytes at offset 100
    print(f"First 64 bytes: {data}")
    print(f"At offset 100: {data_at_100}")
```

The key property: `seek()` and `read()` do not decompress the whole file. Hexz decompresses only the blocks covering your requested byte range.

---

## CLI

```bash
# Build the CLI
make rust

# Pack a file
./target/release/hexz data pack \
    --disk /tmp/hello.bin \
    --output /tmp/hello.hxz \
    --compression lz4

# Inspect the archive
./target/release/hexz data info /tmp/hello.hxz
```

---

## Thin snapshots (dedup across versions)

If you have two versions of a large file:

```bash
# Pack v1
hexz data pack --disk v1.bin --output v1.hxz

# Pack v2, referencing v1 as parent
# Only blocks not already in v1 are written to v2.hxz
hexz data pack --disk v2.bin --output v2.hxz --parent v1.hxz
```

Reading `v2.hxz` is transparent — blocks that are in the parent are fetched from `v1.hxz` automatically. `v2.hxz` only stores the diff.

---

## What to read next

- [Explanation: why CDC vs fixed-size blocks](../explanation/content-defined-chunking.md)
- [Explanation: storage backends](../explanation/storage-backend-design.md)
- [Reference: Python API](../reference/python-api.md)
- [Reference: CLI](../reference/cli-reference.md)
- [Benchmarks](../project-docs/BENCHMARKS.md)
