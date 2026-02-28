# Hexz Roadmap

---

## v0.5.0 — Checkpoint API + Tensor Manifest *(in progress)*

Core checkpoint API on top of the existing storage engine.

| Item | Status |
|---|---|
| Tensor manifest (name → offset/length/dtype/shape in `metadata_offset`) | In progress |
| `hexz.checkpoint.save(state_dict, path, parent=None)` | In progress |
| `hexz.checkpoint.load(path, keys=None, device="cpu")` | In progress |
| `hexz.checkpoint.manifest(path)` | In progress |
| Zero-copy tensor writes: replace `.tobytes()` with `memoryview` buffer protocol | In progress |
| Block-boundary alignment in `save()` — pad each tensor to `block_size` | In progress |
| `cdc: bool = False` on Builder — default off for tensor workloads | In progress |
| `hexz diff a.hxz b.hxz` — block-level comparison, shared vs unique bytes | Planned |
| `hexz ls ./dir/` — list archives with chain structure and unique bytes | Planned |
| Rename SnapshotStream::Disk/Memory → Primary/Secondary | **Done** |

---

## v0.6.0 — Safetensors + GGUF Native Support *(next)*

Native tensor format parsing and tensor-boundary chunking.

| Item | Description |
|---|---|
| Safetensors parser (`crates/core/src/format/safetensors.rs`) | Parse 8-byte header length + JSON + tensor data; `IndexMap` for order preservation |
| GGUF parser (`crates/core/src/format/gguf.rs`) | Parse magic, version, metadata KV pairs, tensor info array |
| `store_safetensors` op (`crates/ops/src/safetensors.rs`) | Chunk at tensor boundaries, pad to block_size, write manifest |
| `extract_safetensors` op | Reconstruct safetensors header from manifest, write tensor bytes in order |
| `hexz store` CLI command | `hexz store INPUT OUTPUT [--base BASE] [--compression zstd]` |
| `hexz extract` CLI command | `hexz extract INPUT [OUTPUT] [--tensor NAME]` |
| `hexz.checkpoint.convert(src, dst, *, base=None)` Python API | No PyTorch required |
| `hexz.checkpoint.extract(src, dst=None, *, tensor=None)` Python API | Round-trip to safetensors |
| `hexz inspect` updated | Show tensor manifest (names, shapes, dtypes, storage mode) |
| `indexmap` workspace dep | Add to `Cargo.toml` |

---

## v0.7.0 — XOR Delta Compression

The core algorithm that makes checkpoint chains dramatically more efficient.

| Item | Description |
|---|---|
| XOR delta write path | Align tensors by name, XOR raw bytes, zstd-compress result, tag as `XorDelta` in index |
| XOR delta read path | Decompress delta, read parent tensor, XOR to reconstruct |
| SIMD XOR acceleration | `portable_simd` or AVX2 intrinsics for ~memory-bandwidth-speed XOR |
| `--no-xor-delta` flag | Opt-out; falls back to raw block storage with BLAKE3 dedup |
| Shape mismatch handling | Warn and fall back to raw storage when shapes or dtypes differ |
| Benchmark: XOR delta vs CDC vs naive zstd on real fine-tuned models | Publish results |

---

## v0.8.0 — Read Path Performance

Known bottlenecks in the read path.

| Issue | Description |
|---|---|
| [#113](https://github.com/hexz-org/hexz/issues/113) | Replace `block_on` in S3/HTTP backends with proper async runtime |
| [#114](https://github.com/hexz-org/hexz/issues/114) | Shard the page cache Mutex (currently a single global lock) |
| [#56](https://github.com/hexz-org/hexz/issues/56) | `Writer.add_bytes()` without temporary file |
| — | Mmap backend: zero-copy `Bytes` from mmap instead of `copy_from_slice` per block |
| — | S3 backend: take ownership of response bytes directly, avoid `copy_from_slice` |
| — | FileBackend: pool `BytesMut` allocations in `read_exact` |

---

## v0.9.0 — Write Path Performance

| Item | Description |
|---|---|
| `compress_into / encrypt_into` | Zero-copy compression/encryption into caller-owned buffers |
| Chunker buffer reuse | `FixedChunker` and `StreamChunker` reuse internal buffers |
| Zstd encoder pooling | Reusable compressor/decompressor, avoid per-call setup cost |
| Pre-sized dedup map | Use file size hint to pre-allocate the hash table |

---

## v0.10.0 — Reliability

| Issue | Description |
|---|---|
| [#86](https://github.com/hexz-org/hexz/issues/86) | Error recovery: retry with exponential backoff, circuit breaker |
| [#101](https://github.com/hexz-org/hexz/issues/101) | Fuzz testing for format parsers (safetensors, GGUF, hxz) |
| [#102](https://github.com/hexz-org/hexz/issues/102) | Security audit and dependency review |
| [#69](https://github.com/hexz-org/hexz/issues/69) | Edge case tests: empty archive, single block, very large file |
| [#68](https://github.com/hexz-org/hexz/issues/68) | Integration tests: concurrent access patterns |

---

## v1.0.0 — Stable API

| Item | Description |
|---|---|
| Stable `hexz.checkpoint.*` API with semver guarantees | No breaking changes after 1.0 |
| `hexz.lock` file | JSON file tracking `{version, root_hash, size}` for reproducible model pinning |
| [#128](https://github.com/hexz-org/hexz/issues/128) | Performance regression testing in CI |
| [#116](https://github.com/hexz-org/hexz/issues/116) | Cap dedup map memory for very large packs |

---

## Backlog (unscheduled)

| Item | Description |
|---|---|
| HuggingFace Hub integration | `hexz push` / `hexz pull` directly to HF Hub |
| Azure Blob Storage backend | |
| Google Cloud Storage backend | |
| Distributed checkpoint storage | Shared dedup index across machines |
| GPU-accelerated XOR | CUDA kernels for XOR delta on VRAM tensors |
| Training loop callback | PyTorch callback: `hexz.checkpoint.save` every N steps with auto parent chaining |
| GGUF round-trip | Export `.hxz` back to GGUF |

---

## What's not on the roadmap

- FUSE mounting — deprioritized; useful for VM disk images but not core to the model checkpoint use case
- VM management (boot, install, snap, commit) — removed
- Windows FUSE support — not prioritized
- Managed SaaS / hosted service — possible future direction, no concrete plan
- Competing with WebDataset/StreamingDataset on sequential training data throughput — not the target use case
