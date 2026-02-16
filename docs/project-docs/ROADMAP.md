# Hexz Development Roadmap

> **Last Updated:** 2026-02-15
> **Current Release:** v0.1.4
> **Status:** Core engine stable. Python wheels and CLI binaries shipping for Linux, macOS, and Windows.

---

## What's Done

Implemented and shipping:

- **Core format**: Seekable block-based snapshots with two-level index
- **Compression**: LZ4, Zstd, Zstd dictionary training (`--train-dict`)
- **Encryption**: AES-256-GCM per-block encryption with PBKDF2 key derivation
- **Signing**: Ed25519 keypair generation, signing, and verification
- **Deduplication**: BLAKE3-based block dedup, FastCDC, DCAM parameter optimization
- **Thin snapshots**: Parent-child delta storage
- **Python bindings**: Reader, AsyncReader, Writer, Dataset (PyTorch), ArrayView, build, convert, inspect
- **CLI**: `data pack/build/info/convert/analyze`, `vm boot/install/mount/snap/commit`, `sys doctor/bench/serve/keygen/sign/verify`
- **Storage backends**: Local file, mmap, S3 (+ S3-compatible), HTTP range requests
- **Performance**: Parallel decompression (Rayon), fixed-window prefetch, sharded LRU cache, GIL release
- **Conversion**: tar, HDF5, WebDataset
- **FUSE**: Read-only mount with copy-on-write overlay
- **Cross-platform**: Linux (x86_64, aarch64), macOS (x86_64, ARM), Windows (x86_64)
- **Publishing**: PyPI (`pip install hexz`), crates.io (`cargo install hexz-cli`), GitHub releases

---

## v0.2.0 — Performance

Focus: Fix the known bottlenecks in the read path.

| Issue | Description |
|---|---|
| [#113](https://github.com/hexz-org/hexz/issues/113) | Replace `block_on` in S3/HTTP storage with proper async runtime |
| [#114](https://github.com/hexz-org/hexz/issues/114) | Shard the page cache Mutex (single global lock) |
| [#115](https://github.com/hexz-org/hexz/issues/115) | Reduce per-call locking overhead in Python loader cursor |
| [#122](https://github.com/hexz-org/hexz/issues/122) | Reusable buffer pool for decompression (reduce allocator pressure) |
| [#73](https://github.com/hexz-org/hexz/issues/73) | Profile hot paths with perf/flamegraph |
| [#56](https://github.com/hexz-org/hexz/issues/56) | `Writer.add_bytes()` without temporary file |

---

## v0.3.0 — Krapivin Hash Table (Cutting Edge!)

Focus: First production implementation of the 2025 hash table breakthrough.

| Issue | Description |
|---|---|
| **Krapivin Hash Table** | Implement [Optimal Bounds for Open Addressing Without Reordering](https://arxiv.org/abs/2501.02305) for deduplication map — 15-20% memory savings, O(log² n) worst-case lookups, disproves Yao's 40-year conjecture |

---

## v0.4.0 — Reliability & Testing

Focus: Error handling, hardening, and confidence in correctness.

| Issue | Description |
|---|---|
| [#86](https://github.com/hexz-org/hexz/issues/86) | Error recovery: retry with exponential backoff, circuit breaker |
| [#101](https://github.com/hexz-org/hexz/issues/101) | Fuzz testing for parsers and format |
| [#102](https://github.com/hexz-org/hexz/issues/102) | Security audit and dependency review |
| [#127](https://github.com/hexz-org/hexz/issues/127) | SLA/reliability testing suite |
| [#144](https://github.com/hexz-org/hexz/issues/144) | Improve mutation testing for Python and Rust |
| [#69](https://github.com/hexz-org/hexz/issues/69) | Edge case tests: empty dataset, single-file, very large file |
| [#68](https://github.com/hexz-org/hexz/issues/68) | Integration test: concurrent access patterns |

---

## v0.5.0 — Hugging Face & Cloud Backends

Focus: Integrate with the ML ecosystem and expand cloud storage.

| Issue | Description |
|---|---|
| [#139](https://github.com/hexz-org/hexz/issues/139) | Hugging Face Datasets integration |
| [#140](https://github.com/hexz-org/hexz/issues/140) | PyTorch Hub integration |
| [#88](https://github.com/hexz-org/hexz/issues/88) | Azure Blob Storage backend |
| [#89](https://github.com/hexz-org/hexz/issues/89) | Google Cloud Storage backend |
| [#64](https://github.com/hexz-org/hexz/issues/64) | Integration test: full PyTorch training loop |

---

## v0.6.0 — Snapshot Management

Focus: Tools for working with multiple snapshots.

| Issue | Description |
|---|---|
| [#47](https://github.com/hexz-org/hexz/issues/47) | `hexz data diff` — compare two snapshots |
| [#95](https://github.com/hexz-org/hexz/issues/95) | `hexz data merge` — merge two snapshots |
| [#96](https://github.com/hexz-org/hexz/issues/96) | `hexz data repair` — repair corrupted snapshots |
| [#135](https://github.com/hexz-org/hexz/issues/135) | Streaming writer / append mode |
| [#136](https://github.com/hexz-org/hexz/issues/136) | Snapshot versioning and lineage tracking |
| [#80](https://github.com/hexz-org/hexz/issues/80) | DCAM `--estimate-savings` dry-run mode |

---

## v0.7.0 — More Frameworks & Optimization

Focus: Broader ML framework support and second-pass performance work.

| Issue | Description |
|---|---|
| [#50](https://github.com/hexz-org/hexz/issues/50) | TensorFlow `tf.data.Dataset` wrapper |
| [#91](https://github.com/hexz-org/hexz/issues/91) | JAX / grain dataset support |
| [#74](https://github.com/hexz-org/hexz/issues/74) | Adaptive prefetch based on access patterns |
| [#75](https://github.com/hexz-org/hexz/issues/75) | Batch small reads into larger backend requests |
| [#78](https://github.com/hexz-org/hexz/issues/78) | Further reduce GIL contention in Python read path |
| [#79](https://github.com/hexz-org/hexz/issues/79) | Zero-copy buffer sharing with NumPy/PyTorch |

---

## v1.0.0 — Stable Release

Focus: Stable public API, production hardening, security.

| Issue | Description |
|---|---|
| [#129](https://github.com/hexz-org/hexz/issues/129) | Access control and audit logging |
| [#84](https://github.com/hexz-org/hexz/issues/84) | Key rotation, multiple keys, keychain integration |
| [#106](https://github.com/hexz-org/hexz/issues/106) | CI benchmark baseline comparison |
| [#128](https://github.com/hexz-org/hexz/issues/128) | Performance regression testing in CI |
| [#116](https://github.com/hexz-org/hexz/issues/116) | Cap deduplication map memory for very large packs |
| Stable API guarantees, semver compliance |

---

## Backlog

Not assigned to a version. Will be pulled in as priorities shift.

### Features

| Issue | Description |
|---|---|
| [#97](https://github.com/hexz-org/hexz/issues/97) | Named streams (multi-stream in one snapshot) |
| [#98](https://github.com/hexz-org/hexz/issues/98) | Virtual concatenation of multiple snapshots |
| [#99](https://github.com/hexz-org/hexz/issues/99) | Delta encoding / binary diff between snapshots |
| [#100](https://github.com/hexz-org/hexz/issues/100) | Hot/cold data tiering |
| [#83](https://github.com/hexz-org/hexz/issues/83) | Per-block compression selection |
| [#87](https://github.com/hexz-org/hexz/issues/87) | Structured logging and Prometheus metrics |
| [#93](https://github.com/hexz-org/hexz/issues/93) | CLI interactive TUI mode |
| [#137](https://github.com/hexz-org/hexz/issues/137) | Bandwidth throttling and rate limiting |
| [#45](https://github.com/hexz-org/hexz/issues/45) | Handle parent encryption in thin snapshots |
| [#90](https://github.com/hexz-org/hexz/issues/90) | MinIO optimization |
| [#81](https://github.com/hexz-org/hexz/issues/81) | Dedup statistics in inspect output |
| [#82](https://github.com/hexz-org/hexz/issues/82) | Zstd dictionary training improvements |
| [#55](https://github.com/hexz-org/hexz/issues/55) | Python `verify()` checksum-only path |

### Testing

| Issue | Description |
|---|---|
| [#107](https://github.com/hexz-org/hexz/issues/107) | Large-scale tests: 1TB+ snapshot, 100M+ samples |
| [#108](https://github.com/hexz-org/hexz/issues/108) | Stress test: 1000+ concurrent readers |
| [#70](https://github.com/hexz-org/hexz/issues/70) | Edge case tests: binary vs text, high vs low entropy |
| [#65](https://github.com/hexz-org/hexz/issues/65) | Integration test: S3 backend with retry logic |
| [#66](https://github.com/hexz-org/hexz/issues/66) | Integration test: HTTP backend with connection failures |
| [#67](https://github.com/hexz-org/hexz/issues/67) | Integration test: FUSE mount operations |
| [#77](https://github.com/hexz-org/hexz/issues/77) | Adaptive cache sizing and eviction policy |
| [#118](https://github.com/hexz-org/hexz/issues/118) | Benchmark/document memory for large packs |
| [#54](https://github.com/hexz-org/hexz/issues/54) | Profile and optimize async I/O paths |

### Documentation

| Issue | Description |
|---|---|
| [#104](https://github.com/hexz-org/hexz/issues/104) | Issue and PR templates |
| [#105](https://github.com/hexz-org/hexz/issues/105) | Example notebooks (Jupyter, Colab) |
| [#59](https://github.com/hexz-org/hexz/issues/59) | Migration guides: tar, HDF5, WebDataset |
| [#60](https://github.com/hexz-org/hexz/issues/60) | Troubleshooting common issues doc |
| [#126](https://github.com/hexz-org/hexz/issues/126) | Production deploy guide |
| [#125](https://github.com/hexz-org/hexz/issues/125) | Landing page with comparison benchmarks |
| [#124](https://github.com/hexz-org/hexz/issues/124) | ROI/cost saving calculator |
| [#132](https://github.com/hexz-org/hexz/issues/132) | Quick start video |
| [#130](https://github.com/hexz-org/hexz/issues/130) | Multi-cloud cost optimization guide |

### Future Ideas

| Issue | Description |
|---|---|
| [#46](https://github.com/hexz-org/hexz/issues/46) | Firecracker boot orchestration |
| [#131](https://github.com/hexz-org/hexz/issues/131) | Managed service / SaaS offering design |
| [#133](https://github.com/hexz-org/hexz/issues/133) | VS Code / PyCharm plugins |
| [#134](https://github.com/hexz-org/hexz/issues/134) | Dataset registry / catalog |
| [#141](https://github.com/hexz-org/hexz/issues/141) | Anonymous usage telemetry (opt-in) |
| [#142](https://github.com/hexz-org/hexz/issues/142) | Cost analytics dashboard |

### Research

- GPU-accelerated decompression (nvCOMP / CUDA kernels for LZ4/Zstd)
- Learned compression and index structures
- Distributed multi-writer coordination and global deduplication
- Content-addressable storage / P2P dataset sharing

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup.

**Good first issues:** [#104](https://github.com/hexz-org/hexz/issues/104), [#118](https://github.com/hexz-org/hexz/issues/118), [#132](https://github.com/hexz-org/hexz/issues/132)
