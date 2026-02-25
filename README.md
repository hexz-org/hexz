# Hexz

Hexz is a storage engine for ML model checkpoints and large binary blobs. It uses content-defined chunking (CDC) and a two-level index to enable deduplication across checkpoint versions and O(log N) random access to any byte range — including individual tensors — without reading the whole file.

Backends: local files, HTTP/HTTPS, and S3.

---

## Installation

```bash
# Python library
pip install hexz

# CLI tool
cargo install hexz-cli
```

---

## CLI

```
hexz pack <output.hxz> [--disk <file>] [--memory <file>] [--compression lz4|zstd] [--cdc] [--block-size N] [--encrypt] [--silent]
```
Pack a disk image or memory dump into a Hexz archive.

```
hexz inspect <archive.hxz> [--json]
```
Read archive header and index without decompressing. Shows compression ratio, block count, metadata.

```
hexz build <source/> <output.hxz> [--profile <name>] [--cdc] [--encrypt] [--memory <file>]
```
Recursively build an archive from a directory.

```
hexz commit <base.hxz> <overlay.bin> <output.hxz> [--thin] [--compression lz4|zstd] [--flatten] [--message <msg>]
```
Finalize a copy-on-write overlay into a new immutable snapshot. `--thin` stores only the delta and references the parent.

```
hexz convert <format> <input> <output.hxz> [--compression lz4|zstd] [--block-size N] [--profile <name>] [--silent]
```
Convert an external format (tar, hdf5, webdataset) to Hexz. Format is auto-detected from extension if omitted.

```
hexz mount <archive.hxz> <mountpoint> [--overlay <file>] [--rw] [--daemon] [--cache-size 1G]
```
Mount a Hexz archive as a FUSE filesystem. *(requires `fuse` feature)*

```
hexz unmount <mountpoint>
```
Unmount a previously mounted archive.

```
hexz boot <archive.hxz> [--ram 4G] [--backend qemu|firecracker] [--persist <overlay>] [--no-graphics] [--vnc]
```
Boot a VM directly from a snapshot using a copy-on-write overlay. *(requires `fuse` feature)*

```
hexz install <image.iso> <output.hxz> [--primary-size 10G] [--ram 4G] [--no-graphics] [--cdc]
```
Run an OS installer from an ISO and capture the result into a snapshot. *(requires `fuse` feature)*

```
hexz snap <qmp-socket> <base.hxz> <overlay.bin> <output.hxz>
```
Trigger a live snapshot of a running VM via the QMP socket.

```
hexz serve <archive.hxz> [--port 8080] [--daemon] [--s3] [--nbd]
```
Serve a snapshot over HTTP with byte-range support. *(requires `server` feature)*

```
hexz keygen [--output-dir <dir>]
```
Generate an Ed25519 keypair for signing archives. *(requires `signing` feature)*

```
hexz sign <private.pem> <archive.hxz>
hexz verify <public.pem> <archive.hxz>
```
Sign or verify an archive. *(requires `signing` feature)*

---

## Python API

### `hexz.open(path, *, mode="r", **options) → Reader | Writer`

Open a snapshot for reading or writing. `path` accepts local paths, HTTP URLs, and S3 URIs.

Read options: `cache_size` (e.g. `"1G"`), `prefetch` (bool), `s3_region`, `endpoint_url`, `allow_restricted`.
Write options: `compression` (`"lz4"` or `"zstd"`), `block_size`, `packing` (`"fast"`, `"balanced"`, `"tight"`).

---

### `hexz.Reader`

```python
Reader(path, *, cache_size=None, prefetch=True, s3_region=None, endpoint_url=None, allow_restricted=False)
```

| Method / Property | Description |
|---|---|
| `read(size=-1, *, offset=None, buffer=None)` | Read bytes from current position or at `offset`. Pass a `bytearray` to `buffer` for zero-copy fills. |
| `read_range(start, end)` | Read byte range `[start, end)`. |
| `readinto(buffer)` | Fill a writable buffer from current position. |
| `seek(offset, whence=0)` | Seek to a position (0=absolute, 1=relative, 2=from end). |
| `tell()` | Current read position. |
| `iter_chunks(chunk_size=1M)` | Iterate over the snapshot in fixed-size chunks with a reused buffer. |
| `analyze()` | Return an `AnalysisReport` with deduplication statistics. |
| `size` | Total uncompressed size in bytes. |
| `metadata` | `Metadata` object with version, compression, block count, etc. |
| `reader[start:end]` | Slice notation for random access. |
| `close()` | Release resources. |

---

### `hexz.AsyncReader`

Same constructor as `Reader`. Use as `async with hexz.AsyncReader(path) as reader`.

| Method | Description |
|---|---|
| `await read(size=None, *, offset=None)` | Read bytes asynchronously. |
| `await seek(offset, whence=0)` | Seek asynchronously. |
| `tell()` | Current position. |
| `size()` | Total size in bytes. |

---

### `hexz.Writer`

```python
Writer(path, *, compression="lz4", packing="balanced", block_size=65536, dedup=True, cdc=False, parent=None)
```

`parent` can be a single path or list of paths to enable cross-file deduplication.

| Method | Description |
|---|---|
| `add(source)` | Add a file path, `bytes`, or NumPy array. Dispatches automatically. |
| `add_file(path, *, kind=None)` | Add a file by path. |
| `add_bytes(data)` | Add raw bytes. |
| `add_array(array)` | Add a NumPy array. |
| `add_metadata(dict)` | Store a JSON-serializable dict as snapshot metadata. |
| `merge_overlay(*, base, overlay, thin=False)` | Merge a copy-on-write overlay with a base snapshot. |
| `write(data)` | Write bytes (file-like API). |
| `finalize()` | Write index and flush. Called automatically on context manager exit. |
| `bytes_written` | Total bytes written so far. |
| `tell()` | Current write position. |

---

### `hexz.checkpoint`

Tensor-aware save/load built on top of the storage engine. Requires PyTorch.

#### `hexz.checkpoint.save(state_dict, path, *, compression="zstd", block_size=131072, parent=None) → Metadata`

Save a PyTorch `state_dict`. Pass `parent` to store only the delta against a previous checkpoint. Supports tensors (`float16`, `float32`, `float64`, `bfloat16`, `int8`–`int64`, `uint8`, `bool`) and scalars (`int`, `float`, `bool`, `str`).

#### `hexz.checkpoint.load(path, *, keys=None, device="cpu") → dict`

Load tensors and scalars. Pass `keys` to load a subset without reading the rest of the file.

#### `hexz.checkpoint.manifest(path) → dict`

Read tensor names, shapes, dtypes, and byte offsets without loading any data.

---

### `hexz.inspect(path) → Metadata`

Inspect a snapshot header and index. Returns a `Metadata` object with properties:

`version`, `compression`, `primary_size`, `secondary_size`, `size_compressed`, `block_size`, `num_blocks`, `encrypted`, `signed`, `compression_ratio`, `is_compatible`, `compatibility_status`.

---

### `hexz.verify(path, *, checksum=True, structure=True, public_key=None) → bool`

Verify snapshot integrity. Optionally verifies a cryptographic signature.

---

### `hexz.convert(input, output, *, format=None, compression="lz4", block_size=None, profile=None) → Metadata`

Convert a tar, HDF5, or WebDataset file to a Hexz snapshot. Format is auto-detected from extension (`.tar`, `.tar.gz`, `.tgz`, `.h5`, `.hdf5`, `.wds`).

---

### `hexz.build(source, output, *, profile="generic", **overrides) → Metadata`

Build a snapshot from a file or directory using a named profile.

Available profiles: `ml`, `eda`, `embedded`, `generic`, `archival`.

---

### `hexz.read_array(source, *, offset=0, shape, dtype="float32", order="C", copy=True) → np.ndarray`

Read a NumPy array from a snapshot at a byte offset.

---

### `hexz.write_array(dest, array, *, compression="lz4") → int`

Write a NumPy array to a new snapshot. Returns bytes written.

---

### `hexz.ArrayView`

```python
ArrayView(path, shape, dtype="float32", offset=0)
```

Random-access view into array data stored in a snapshot. Supports integer indexing and slicing (`view[0:100]`) without loading the full array. Use as a context manager.

---

## License

Licensed under [Apache 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your option.
