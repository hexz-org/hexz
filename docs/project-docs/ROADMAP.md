# Hexz Roadmap

> Last Updated: 2026-02-17
> Current version: v0.4.0

---

## What's shipped

- **Core format**: Seekable block-based archive with two-level index, O(log N) random access
- **Compression**: LZ4, Zstd (levels 1-9), Zstd dictionary training
- **Encryption**: AES-256-GCM per-block, PBKDF2 key derivation, AES-NI accelerated
- **Signing**: Ed25519 keypair generation, signing, verification
- **Deduplication**: BLAKE3 block hashing, FastCDC content-defined chunking, DCAM parameter optimization
- **Thin snapshots**: Parent-child delta storage via `parent_path` in header
- **Python bindings**: Reader, Writer, AsyncReader, Dataset (PyTorch), ArrayView, build, convert, inspect, verify, keygen, sign
- **CLI**: `data pack/build/info/convert/analyze`, `vm boot/install/mount/snap/commit`, `sys doctor/bench/serve/keygen/sign/verify`
- **Storage backends**: Local file, mmap, S3 (+ S3-compatible endpoints), HTTP/HTTPS byte-range
- **Performance**: Parallel decompression via Rayon, fixed-window prefetch, sharded LRU cache, GIL release during I/O
- **Format conversion**: tar, HDF5, WebDataset input
- **FUSE**: Read-only mount with copy-on-write overlay (Linux)
- **Platforms**: Linux (x86_64, aarch64), macOS (x86_64, ARM), Windows (x86_64, core only — FUSE/mmap not validated)

---

## v0.5.0 — Checkpoint API + Tensor Manifest

The pivot toward checkpoint versioning as the primary use case. No new format changes — this is Python API work on top of what's already in the format.

| Item | Description |
|---|---|
| **Tensor manifest** | Write tensor name → (offset, length, dtype, shape) as a msgpack blob into the existing `metadata_offset` slot in the header |
| **`hexz.checkpoint.save(state_dict, path, parent=None)`** | Python API: writes tensors sequentially, builds manifest, sets `parent_path` if provided |
| **`hexz.checkpoint.load(path, keys=None)`** | Python API: reads manifest, fetches requested tensors by byte range, returns dict |
| **`hexz checkpoint diff a.hxz b.hxz`** | CLI: compare block hashes between two files, report shared vs unique bytes and storage savings |
| **`hexz checkpoint ls ./dir/`** | CLI: list archives, show chain structure and unique bytes on disk |
| **safetensors import** | `hexz.checkpoint.from_safetensors(path, output, parent=None)` |
| **Rename SnapshotStream::Disk/Memory** | → Primary/Secondary. VM use case unchanged, removes VM-specific naming from ML-facing API |

---

## v0.6.0 — Read Path Performance

Known bottlenecks in the read path. Issues already tracked:

| Issue | Description |
|---|---|
| [#113](https://github.com/hexz-org/hexz/issues/113) | Replace `block_on` in S3/HTTP backends with proper async runtime |
| [#114](https://github.com/hexz-org/hexz/issues/114) | Shard the page cache Mutex (currently a single global lock) |
| [#115](https://github.com/hexz-org/hexz/issues/115) | Reduce per-call locking in Python loader cursor |
| [#122](https://github.com/hexz-org/hexz/issues/122) | Reusable buffer pool for decompression |
| [#56](https://github.com/hexz-org/hexz/issues/56) | `Writer.add_bytes()` without temporary file |

---

## v0.7.0 — Write Path Performance

Buffer reuse and zero-copy write APIs:

| Item | Description |
|---|---|
| **compress_into / encrypt_into** | Zero-copy compression/encryption APIs writing into caller-owned buffers |
| **Chunker buffer reuse** | FixedChunker and StreamChunker reuse internal buffers instead of allocating per chunk |
| **Monomorphized chunker dispatch** | Enum dispatch replacing `Box<dyn Chunker>` |
| **Pre-sized dedup map** | Use file size hint to pre-allocate the dedup hash table |
| **Zstd encoder pooling** | Reusable compressor/decompressor to avoid per-call setup |

---

## v0.8.0 — Reliability

| Issue | Description |
|---|---|
| [#86](https://github.com/hexz-org/hexz/issues/86) | Error recovery: retry with exponential backoff, circuit breaker |
| [#101](https://github.com/hexz-org/hexz/issues/101) | Fuzz testing for parsers and format |
| [#102](https://github.com/hexz-org/hexz/issues/102) | Security audit and dependency review |
| [#127](https://github.com/hexz-org/hexz/issues/127) | SLA/reliability testing suite |
| [#69](https://github.com/hexz-org/hexz/issues/69) | Edge case tests: empty archive, single block, very large file |
| [#68](https://github.com/hexz-org/hexz/issues/68) | Integration tests: concurrent access patterns |

---

## v0.9.0 — Snapshot Management

| Issue | Description |
|---|---|
| [#95](https://github.com/hexz-org/hexz/issues/95) | `hexz data merge` — merge two archives |
| [#96](https://github.com/hexz-org/hexz/issues/96) | `hexz data repair` — repair corrupted archives |
| [#135](https://github.com/hexz-org/hexz/issues/135) | Streaming writer / append mode |
| [#136](https://github.com/hexz-org/hexz/issues/136) | Snapshot versioning and lineage tracking |

---

## v1.0.0 — Stable API

| Item | Description |
|---|---|
| Stable public API with semver guarantees | No breaking changes after 1.0 |
| [#129](https://github.com/hexz-org/hexz/issues/129) | Access control and audit logging |
| [#84](https://github.com/hexz-org/hexz/issues/84) | Key rotation and keychain integration |
| [#128](https://github.com/hexz-org/hexz/issues/128) | Performance regression testing in CI |
| [#116](https://github.com/hexz-org/hexz/issues/116) | Cap dedup map memory for very large packs |

---

## Backlog (unscheduled)

### Features

| Issue | Description |
|---|---|
| [#97](https://github.com/hexz-org/hexz/issues/97) | Named streams (multi-stream in one archive) |
| [#98](https://github.com/hexz-org/hexz/issues/98) | Virtual concatenation of multiple archives |
| [#99](https://github.com/hexz-org/hexz/issues/99) | Delta encoding / binary diff between archives |
| [#83](https://github.com/hexz-org/hexz/issues/83) | Per-block compression algorithm selection |
| [#87](https://github.com/hexz-org/hexz/issues/87) | Structured logging and Prometheus metrics |
| [#45](https://github.com/hexz-org/hexz/issues/45) | Handle parent encryption in thin snapshots |
| [#81](https://github.com/hexz-org/hexz/issues/81) | Dedup statistics in inspect output |
| Cross-file external dedup index | Shared dedup index for unrelated archives — required for the full checkpoint dedup story |
| Azure Blob Storage backend | |
| Google Cloud Storage backend | |
| HuggingFace Datasets integration | |
| TensorFlow `tf.data.Dataset` wrapper | |
| JAX / grain dataset support | |

### Testing

| Issue | Description |
|---|---|
| [#107](https://github.com/hexz-org/hexz/issues/107) | Large-scale tests: 1TB+ archive, 100M+ samples |
| [#64](https://github.com/hexz-org/hexz/issues/64) | End-to-end PyTorch training loop integration test |
| WebDataset benchmark | Run the pending WebDataset comparison before publishing competitive claims |

### Research

- GPU-accelerated decompression (nvCOMP / CUDA kernels for LZ4/Zstd)
- Distributed multi-writer coordination and global deduplication
- **Krapivin et al. (2025)** — Optimal Bounds for Open Addressing Without Reordering: Evaluated for the dedup hash table. Standard HashMap with identity hasher (leveraging BLAKE3's uniform distribution) outperformed the elastic hash table by 3-6× on lookup-heavy workloads. Identity hasher approach adopted in v0.4.0.

---

## What's not on the roadmap

- A managed SaaS / hosted service — possible future direction but no concrete plan
- GPU training pipeline features competing with WebDataset/StreamingDataset — not the target
- Windows FUSE support — not prioritized
