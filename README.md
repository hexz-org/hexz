# Hexz

Seekable, deduplicated compression format built in Rust. Pack large datasets, ML checkpoints, disk images, and binary blobs into compressed, indexed archives with instant random access — no full decompression needed.

**[Documentation](https://hexz-org.github.io/hexz/) · [PyPI](https://pypi.org/project/hexz/) · [crates.io](https://crates.io/crates/hexz-cli) · [Releases](https://github.com/hexz-org/hexz/releases)**

---

## Install

```bash
pip install hexz           # Python library
cargo install hexz-cli     # CLI tool
```

Pre-built binaries for Linux, macOS, and Windows are available on the [releases page](https://github.com/hexz-org/hexz/releases).

---

## What Hexz Can Do

### Archive Operations

| Command | Description |
|---|---|
| `hexz pack` | Pack a disk image or memory dump into a Hexz archive |
| `hexz inspect` | Read archive header and index without decompressing |
| `hexz diff` | Show differences in a copy-on-write overlay |
| `hexz ls` | List files inside an archive |
| `hexz build` | Pack with profile-based presets (ml, eda, embedded, generic) |
| `hexz convert` | Convert tar, HDF5, or WebDataset files to Hexz |

### Virtual Machine Operations

| Command | Description |
|---|---|
| `hexz boot` | Boot a VM directly from a snapshot (copy-on-write overlay) |
| `hexz install` | Install an OS from ISO and capture the result as a snapshot |
| `hexz snap` | Trigger a live VM snapshot via QMP socket |
| `hexz commit` | Commit overlay changes into a new immutable snapshot |
| `hexz mount` | Mount a snapshot as a FUSE filesystem |
| `hexz unmount` | Unmount a previously mounted archive |

### System & Diagnostics

| Command | Description |
|---|---|
| `hexz doctor` | Check system compatibility (FUSE, QEMU, DNS) |
| `hexz serve` | Serve a snapshot over HTTP or NBD with byte-range support |
| `hexz keygen` | Generate an Ed25519 keypair for signing |
| `hexz sign` | Sign an archive |
| `hexz verify` | Verify an archive signature |

Run `hexz --help` or `hexz COMMAND --help` for full usage.

---

## Python API

### Opening snapshots

```python
import hexz

# Read
with hexz.open("model.hxz") as r:
    data = r.read(1024, offset=4096)

# Write
with hexz.open("out.hxz", mode="w", compression="zstd") as w:
    w.add_file("model.bin")
    w.add_metadata({"epoch": 10, "loss": 0.042})
```

`hexz.open(path, *, mode="r", **options)` accepts local paths, HTTP URLs, and S3 URIs.

### Reader

```python
Reader(path, *, cache_size=None, prefetch=True, s3_region=None, endpoint_url=None)
```

| Method / Property | Description |
|---|---|
| `read(size=-1, *, offset=None, buffer=None)` | Read bytes; pass a `bytearray` to `buffer` for zero-copy fills |
| `read_range(start, end)` | Read byte range `[start, end)` |
| `seek(offset, whence=0)` | Seek (0=absolute, 1=relative, 2=from end) |
| `iter_chunks(chunk_size=1M)` | Iterate over the snapshot in fixed-size chunks |
| `analyze()` | Return deduplication statistics |
| `reader[start:end]` | Slice notation for random access |
| `size` | Total uncompressed size in bytes |
| `metadata` | Version, compression, block count, etc. |

### AsyncReader

Same constructor as `Reader`. Use as `async with hexz.AsyncReader(path) as reader`.

```python
data = await reader.read(size=4096, offset=0)
```

### Writer

```python
Writer(path, *, compression="lz4", packing="balanced", block_size=65536, dedup=True, cdc=False, parent=None)
```

`parent` accepts a path or list of paths for cross-file deduplication (thin snapshots).

| Method | Description |
|---|---|
| `add(source)` | Add a file path, `bytes`, or NumPy array |
| `add_file(path)` | Add a file by path |
| `add_bytes(data)` | Add raw bytes |
| `add_array(array)` | Add a NumPy array |
| `add_metadata(dict)` | Attach a JSON-serializable dict |
| `merge_overlay(*, base, overlay, thin=False)` | Merge a copy-on-write overlay with a base snapshot |
| `finalize()` | Write index and flush (called automatically on context exit) |

### ML checkpoints (PyTorch)

```python
import hexz.checkpoint

# Save — pass parent to store only the delta
meta = hexz.checkpoint.save(model.state_dict(), "v2.hxz", parent="v1.hxz")

# Load all tensors
state = hexz.checkpoint.load("v2.hxz", device="cuda")

# Load a subset without reading the full file
state = hexz.checkpoint.load("v2.hxz", keys=["encoder.weight", "decoder.weight"])

# Inspect tensor names and shapes without loading data
manifest = hexz.checkpoint.manifest("v2.hxz")
```

Supports `float16/32/64`, `bfloat16`, `int8`–`int64`, `uint8`, `bool`, and Python scalars.

### NumPy arrays

```python
# Write a NumPy array
hexz.write_array("data.hxz", array, compression="lz4")

# Read it back
arr = hexz.read_array("data.hxz", shape=(1000, 512), dtype="float32")

# Random-access view — no full load
with hexz.ArrayView("data.hxz", shape=(1000, 512), dtype="float32") as view:
    batch = view[0:32]
```

### Utilities

```python
# Inspect without loading data
meta = hexz.inspect("archive.hxz")
# meta.compression_ratio, meta.num_blocks, meta.encrypted, ...

# Verify integrity (optionally check a cryptographic signature)
ok = hexz.verify("archive.hxz", checksum=True, public_key="pub.pem")

# Convert from other formats (auto-detects .tar, .h5, .hdf5, .wds)
hexz.convert("dataset.tar.gz", "dataset.hxz", compression="zstd")

# Build from a directory using a named profile
hexz.build("weights/", "model.hxz", profile="ml")
# Profiles: ml, eda, embedded, generic
```

---

## License

Licensed under [Apache 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.
