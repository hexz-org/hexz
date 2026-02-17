# Hexz

[![Documentation](https://img.shields.io/badge/docs-latest-blue.svg)](https://hexz-org.github.io/hexz/)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)

Hexz is a **seekable, deduplicated, block-compressed binary archive format** written in Rust, with Python bindings. A `.hxz` file stores arbitrary binary data in independently-compressed blocks with a two-level index, enabling random access to any byte range without decompressing the whole archive. Content-defined chunking (FastCDC) means blocks that are identical across different versions of a file are stored only once.

The core primitive: **read any byte range, from any version, without extracting anything, from a single file, with no running daemon.**

---

## What it actually does

- **Random access to compressed data** — O(log N) lookup via a two-level index. Cold access: ~6.6 µs. Warm (cached): ~174 ns.
- **Content deduplication** — FastCDC chunking with BLAKE3 hashing. Identical blocks across versions are stored once. Resilient to insertions/edits (not just appends).
- **Thin snapshots** — A child file references its parent and only stores changed blocks. A chain of checkpoints costs the space of one full copy plus deltas.
- **Pluggable backends** — Local file, mmap, HTTP byte-range requests, S3. The same API works for all of them.
- **Block-level encryption** — AES-256-GCM per block. AES-NI accelerated (~2.1 GB/s). Encryption and signing (Ed25519) can coexist.
- **Python bindings** — PyO3, zero-copy buffer protocol, GIL released during I/O.

## What it does not do

- It is not a training data pipeline. WebDataset, MosaicML StreamingDataset, and similar tools have optimized the PyTorch DataLoader integration deeply. Hexz does not compete there.
- It is not a database. There is no query engine, no schema enforcement, no transactions.
- It is not a platform. There is no server to run, no UI, no access control layer. Those are roadmap items.
- It cannot load arbitrary pickled Python objects. `torch.load` compatibility is not a goal.
- Cross-file deduplication via a shared external index is **not yet implemented**. Deduplication currently works within a single pack operation or via the parent-child thin snapshot chain.

---

## Validated benchmarks

All numbers below are from real benchmark runs. Microbenchmarks run on Intel i7-14700K, 64GB RAM, NVMe SSD, Linux.

**Engine performance (microbenchmarks, single-threaded):**

| Operation | Throughput |
|---|---|
| LZ4 decompress | 32.1 GB/s |
| LZ4 compress | 23.6 GB/s |
| Zstd-3 decompress | 13.4 GB/s |
| Pack LZ4, no CDC | 4.9 GB/s |
| Pack LZ4 + CDC | 1.9 GB/s |
| Sequential read (100MB file) | 9.0 GB/s |
| Random access, cold cache | 6.6 µs |
| Random access, warm cache | 174 ns |

**End-to-end ML data loading (Python, real image datasets):**

Benchmarked against CIFAR-10 (108MB), STL-10 (28MB), CIFAR-100 (18MB) — small datasets, validated on real hardware:

| | Sequential Read | Random Access | Shuffled Epoch |
|---|---|---|---|
| Local files (baseline) | 387 MB/s | 6.2 µs | 360 MB/s |
| HDF5 (LZF) | 56 MB/s | 40.8 µs | 55 MB/s |
| **Hexz (LZ4)** | **1,218 MB/s** | **3.4 µs** | **525 MB/s** |

These are Python-level numbers on a dataset that fits in RAM. At TB scale from S3, the bottleneck is network bandwidth — Hexz won't change that.

**Deduplication (validated):**

50MB base file + 50MB shifted version (1KB inserted at start), packed into one archive:

| Method | Combined size | Dedup of shifted data |
|---|---|---|
| Fixed-size blocks | 100.4 MB | 0% |
| CDC blocks | 54.0 MB | **92.4%** |

Fixed-size dedup breaks when data shifts. CDC doesn't.

---

## Where Hexz is a good fit

**Many versions of the same large binary file.** Model checkpoints, dataset versions, VM snapshots — anything where adjacent versions share most of their content. The thin snapshot chain plus CDC dedup means you store one full copy and pay only for what changed.

**Random access to a specific part without downloading the whole file.** Read a single tensor from a 14GB checkpoint, or a single sample from a 1TB dataset, by fetching only the blocks that contain it — over HTTP or S3.

**Single-file portability.** No directory of chunks, no sidecar metadata files, no daemon. One `.hxz` file contains data, index, and optional manifest.

## Where Hexz is not a good fit

**Single training run, no versioning.** If you pack once and never deduplicate across versions, safetensors (for model weights) or WebDataset (for training data) are simpler and have broader ecosystem support.

**Checkpoints under 1GB.** The overhead is not worth it at small scale.

**Sequential-only streaming at maximum throughput.** WebDataset and MosaicML StreamingDataset have deeply optimized the PyTorch DataLoader prefetch pipeline. They will match or beat Hexz on pure sequential throughput.

**Windows.** The FUSE and mmap paths have not been validated on Windows. Core I/O and the Python bindings work, but don't treat Windows as a supported platform yet.

---

## Installation

```bash
# Python library
pip install hexz

# CLI tool
cargo install hexz-cli
```

## Quick start

```python
import hexz

# Write
with hexz.open("data.hxz", mode="w", compression="lz4") as writer:
    writer.add_file("disk.img")

# Read
with hexz.open("data.hxz") as reader:
    chunk = reader.read(4096)      # read 4KB
    reader.seek(1024 * 1024)       # seek to 1MB offset
    block = reader[100:200]        # slice notation
```

Thin snapshot (only stores changed blocks):

```python
import hexz

# Pack v2, referencing v1 as parent
hexz.build("v2_source/", "v2.hxz", parent="v1.hxz")

# v2.hxz contains only blocks not already in v1.hxz
```

---

## Format demonstration: VM boot

The format is general enough to boot an operating system from a compressed snapshot, paging blocks in on demand. This is not a primary use case — it's a demonstration that the random access latency is low enough for interactive workloads.

```bash
hexz boot ubuntu-22.04.hxz --ram 4G
```

---

## Architecture

| Crate | Purpose |
|---|---|
| `crates/core` | Format, compression, dedup, index, storage backends |
| `crates/loader` | Python bindings via PyO3 |
| `crates/cli` | `hexz` command-line tool |
| `crates/fuse` | FUSE adapter for mounting archives |
| `crates/server` | HTTP server for block streaming |

## Development

All commands go through the Makefile:

```bash
make help          # list all targets
make setup         # one-time setup
make build         # Rust + Python
make test          # all tests
make bench         # benchmarks
make lint          # clippy + format check
```

---

## License

Copyright 2026 Alethic Systems. Apache License, Version 2.0.
