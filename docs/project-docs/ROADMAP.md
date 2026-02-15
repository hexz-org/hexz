# Hexz Development Roadmap

> **Last Updated:** 2026-02-15
> **Current Release:** v0.1.2
> **Status:** Core engine stable. Python wheels and CLI binaries shipping for Linux, macOS, and Windows.

---

## What's Done

These features are implemented and shipping in v0.1.2:

- **Core format**: Seekable block-based snapshots with two-level index
- **Compression**: LZ4, Zstd, Zstd dictionary training (`--train-dict`)
- **Encryption**: AES-256-GCM per-block encryption with PBKDF2 key derivation
- **Signing**: Ed25519 keypair generation, signing, and verification (`hexz sys keygen/sign/verify`)
- **Deduplication**: BLAKE3-based block dedup, FastCDC content-defined chunking, DCAM parameter optimization
- **Thin snapshots**: Parent-child delta storage (v2 only stores blocks that changed from v1)
- **Python bindings**: Reader, AsyncReader, Writer, Dataset (PyTorch), ArrayView, build, convert, inspect
- **CLI**: `data pack`, `data build`, `data info`, `data convert`, `data analyze`, `vm boot/install/mount/snap/commit`, `sys doctor/bench/serve/keygen/sign/verify`
- **Storage backends**: Local file, mmap, S3 (+ S3-compatible), HTTP with range requests
- **Performance**: Parallel decompression (Rayon), fixed-window prefetch, sharded LRU cache, GIL release in all I/O paths
- **Conversion**: tar (.tar/.tar.gz/.tar.bz2/.tar.xz), HDF5, WebDataset
- **FUSE**: Read-only mount with copy-on-write overlay for VMs
- **Cross-platform**: Linux (x86_64, aarch64), macOS (x86_64, Apple Silicon), Windows (x86_64)
- **CI/CD**: Automated wheel builds, binary releases, PyPI publishing, GitHub releases
- **Documentation**: mkdocs site, ADRs, contributing guide, quickstart, examples, API reference
- **Website**: GitHub Pages deployment

---

## v0.2.0 — Performance & Reliability

Focus: Make what exists faster and more robust.

### High Priority

| Issue | Description |
|---|---|
| [#113](https://github.com/hexz-org/hexz/issues/113) | Replace `block_on` in S3/HTTP storage with proper async runtime |
| [#114](https://github.com/hexz-org/hexz/issues/114) | Shard the page cache Mutex (single global lock is a bottleneck) |
| [#115](https://github.com/hexz-org/hexz/issues/115) | Reduce per-call locking overhead in Python loader cursor |
| [#122](https://github.com/hexz-org/hexz/issues/122) | Reusable buffer pool for decompression (reduce allocator pressure) |
| [#86](https://github.com/hexz-org/hexz/issues/86) | Error recovery: retry with exponential backoff, circuit breaker |
| [#127](https://github.com/hexz-org/hexz/issues/127) | SLA/reliability testing suite |
| [#144](https://github.com/hexz-org/hexz/issues/144) | Improve mutation testing for Python and Rust |

### Medium Priority

| Issue | Description |
|---|---|
| [#73](https://github.com/hexz-org/hexz/issues/73) | Profile hot paths with perf/flamegraph, identify cache misses |
| [#74](https://github.com/hexz-org/hexz/issues/74) | Adaptive prefetch based on access patterns (currently fixed window) |
| [#75](https://github.com/hexz-org/hexz/issues/75) | Batch small reads into larger backend requests |
| [#78](https://github.com/hexz-org/hexz/issues/78) | Further reduce GIL contention in Python read path |
| [#79](https://github.com/hexz-org/hexz/issues/79) | Zero-copy buffer sharing with NumPy/PyTorch |
| [#116](https://github.com/hexz-org/hexz/issues/116) | Cap deduplication map memory growth for very large packs |
| [#106](https://github.com/hexz-org/hexz/issues/106) | CI benchmark baseline comparison (detect regressions) |
| [#128](https://github.com/hexz-org/hexz/issues/128) | Performance regression testing in CI |
| [#56](https://github.com/hexz-org/hexz/issues/56) | `Writer.add_bytes()` without temporary file (direct Rust API) |
| [#101](https://github.com/hexz-org/hexz/issues/101) | Fuzz testing for parsers and format (infrastructure exists, needs targets) |
| [#102](https://github.com/hexz-org/hexz/issues/102) | Security audit and dependency review |
| [#69](https://github.com/hexz-org/hexz/issues/69) | Edge case tests: empty dataset, single-file, very large file |
| [#68](https://github.com/hexz-org/hexz/issues/68) | Integration test: concurrent access patterns |
| [#64](https://github.com/hexz-org/hexz/issues/64) | Integration test: full PyTorch training loop (multiple epochs) |

### Low Priority

| Issue | Description |
|---|---|
| [#77](https://github.com/hexz-org/hexz/issues/77) | Adaptive cache sizing and eviction policy |
| [#118](https://github.com/hexz-org/hexz/issues/118) | Benchmark/document memory usage for large packs |
| [#54](https://github.com/hexz-org/hexz/issues/54) | Profile and optimize async I/O paths |
| [#70](https://github.com/hexz-org/hexz/issues/70) | Edge case tests: binary vs text, high vs low entropy |
| [#65](https://github.com/hexz-org/hexz/issues/65) | Integration test: S3 backend with retry logic |
| [#66](https://github.com/hexz-org/hexz/issues/66) | Integration test: HTTP backend with connection failures |
| [#67](https://github.com/hexz-org/hexz/issues/67) | Integration test: FUSE mount operations |

---

## v0.3.0 — Ecosystem Integrations

Focus: Make Hexz work with popular ML frameworks and cloud providers.

### High Priority

| Issue | Description |
|---|---|
| [#139](https://github.com/hexz-org/hexz/issues/139) | Hugging Face Datasets integration (plugin for `datasets` library) |

### Medium Priority

| Issue | Description |
|---|---|
| [#140](https://github.com/hexz-org/hexz/issues/140) | PyTorch Hub integration |
| [#50](https://github.com/hexz-org/hexz/issues/50) | TensorFlow `tf.data.Dataset` wrapper (currently stubbed) |
| [#88](https://github.com/hexz-org/hexz/issues/88) | Azure Blob Storage backend |
| [#89](https://github.com/hexz-org/hexz/issues/89) | Google Cloud Storage backend |
| [#47](https://github.com/hexz-org/hexz/issues/47) | `hexz data diff` — compare two snapshots |
| [#95](https://github.com/hexz-org/hexz/issues/95) | `hexz data merge` — merge two snapshots |
| [#96](https://github.com/hexz-org/hexz/issues/96) | `hexz data repair` — repair corrupted snapshots |
| [#135](https://github.com/hexz-org/hexz/issues/135) | Streaming writer / append mode |
| [#136](https://github.com/hexz-org/hexz/issues/136) | Snapshot versioning and lineage tracking |
| [#80](https://github.com/hexz-org/hexz/issues/80) | DCAM `--estimate-savings` dry-run mode |
| [#129](https://github.com/hexz-org/hexz/issues/129) | Access control and audit logging |

### Low Priority

| Issue | Description |
|---|---|
| [#91](https://github.com/hexz-org/hexz/issues/91) | JAX / grain dataset support |
| [#90](https://github.com/hexz-org/hexz/issues/90) | MinIO optimization and compatibility testing |
| [#81](https://github.com/hexz-org/hexz/issues/81) | Dedup statistics in `hexz data info` output |
| [#82](https://github.com/hexz-org/hexz/issues/82) | Zstd dictionary training improvements |
| [#83](https://github.com/hexz-org/hexz/issues/83) | Per-block compression selection (skip incompressible) |
| [#55](https://github.com/hexz-org/hexz/issues/55) | Python `verify()`: checksum-only path without signature |

---

## v0.4.0 — Advanced Features

Focus: Format extensions and power-user capabilities.

| Issue | Description | Priority |
|---|---|---|
| [#97](https://github.com/hexz-org/hexz/issues/97) | Named streams (multi-stream in one snapshot) | Low |
| [#98](https://github.com/hexz-org/hexz/issues/98) | Virtual concatenation of multiple snapshots | Low |
| [#99](https://github.com/hexz-org/hexz/issues/99) | Delta encoding / binary diff between snapshots | Low |
| [#100](https://github.com/hexz-org/hexz/issues/100) | Hot/cold data tiering | Low |
| [#84](https://github.com/hexz-org/hexz/issues/84) | Key rotation, multiple keys, keychain integration | Low |
| [#45](https://github.com/hexz-org/hexz/issues/45) | Handle parent encryption in thin snapshots | Low |
| [#87](https://github.com/hexz-org/hexz/issues/87) | Structured logging and Prometheus metrics | Low |
| [#93](https://github.com/hexz-org/hexz/issues/93) | CLI interactive TUI mode | Low |
| [#137](https://github.com/hexz-org/hexz/issues/137) | Bandwidth throttling and rate limiting | Low |

---

## Documentation & Community

Ongoing across all versions.

| Issue | Description | Priority |
|---|---|---|
| [#104](https://github.com/hexz-org/hexz/issues/104) | Issue and PR templates | Medium |
| [#105](https://github.com/hexz-org/hexz/issues/105) | Example notebooks (Jupyter, Colab) | Medium |
| [#59](https://github.com/hexz-org/hexz/issues/59) | Migration guides: tar, HDF5, WebDataset | Medium |
| [#60](https://github.com/hexz-org/hexz/issues/60) | Troubleshooting common issues doc | Medium |
| [#126](https://github.com/hexz-org/hexz/issues/126) | Production deploy guide | Medium |
| [#125](https://github.com/hexz-org/hexz/issues/125) | Landing page with comparison benchmarks | Medium |
| [#124](https://github.com/hexz-org/hexz/issues/124) | ROI/cost saving calculator | Medium |
| [#132](https://github.com/hexz-org/hexz/issues/132) | Quick start video | Low |
| [#130](https://github.com/hexz-org/hexz/issues/130) | Multi-cloud cost optimization guide | Low |

---

## Backlog / Future

Ideas that don't have a version target yet.

| Issue | Description | Priority |
|---|---|---|
| [#46](https://github.com/hexz-org/hexz/issues/46) | Firecracker boot orchestration | Low |
| [#107](https://github.com/hexz-org/hexz/issues/107) | Large-scale tests: 1TB+ snapshot, 100M+ samples | Low |
| [#108](https://github.com/hexz-org/hexz/issues/108) | Stress test: 1000+ concurrent readers | Low |
| [#131](https://github.com/hexz-org/hexz/issues/131) | Managed service / SaaS offering design | Low |
| [#133](https://github.com/hexz-org/hexz/issues/133) | VS Code / PyCharm plugins | Low |
| [#134](https://github.com/hexz-org/hexz/issues/134) | Dataset registry / catalog | Low |
| [#141](https://github.com/hexz-org/hexz/issues/141) | Anonymous usage telemetry (opt-in) | Low |
| [#142](https://github.com/hexz-org/hexz/issues/142) | Cost analytics dashboard | Low |

### Research

- GPU-accelerated decompression (nvCOMP / CUDA kernels for LZ4/Zstd)
- Learned compression and index structures
- Distributed multi-writer coordination and global deduplication
- Content-addressable storage / P2P dataset sharing

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup.

**Good first issues:** [#104](https://github.com/hexz-org/hexz/issues/104), [#118](https://github.com/hexz-org/hexz/issues/118), [#132](https://github.com/hexz-org/hexz/issues/132)
