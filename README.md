
# Hexz: High-Performance Data Streaming Engine

**Hexz** is a Rust-based streaming engine designed to eliminate "GPU Starvation" in AI training. It allows PyTorch to stream massive datasets directly from compressed S3 storage into GPU memory, bypassing the Python GIL and OS page cache.

At its core, Hexz is a **seekable, deduplicated compression filesystem**.

> **Why is there VM code in here?**
> Hexz’s engine is so low-latency that it was originally built to **boot entire operating systems** over the network in milliseconds. This has been adapted to load large datasets for machine learning.

## The Problem: The "Data Bottleneck"

Modern GPUs are too fast for standard data loaders.

1. **S3 is high-latency:** Fetching millions of small images one-by-one is slow.
2. **Existing formats are rigid:** Tools like `WebDataset` require "sharding" data into thousands of tar files, breaking random shuffling.
3. **Storage is expensive:** Redundant data (checkpoints, code, synthetic data) wastes PB of storage.

## The Solution: Hexz

Hexz introduces a **Seekable Compressed Archive**.

* **Stream, Don't Shard:** Store your 10TB dataset as a single compressed stream. Hexz can seek to *any* individual sample ID instantly without decompressing the whole block.
* **Native Deduplication:** Identical blocks are stored once. A dataset with 50% redundancy uses 50% less storage and bandwidth automatically.
* **Rust-Powered Concurrency:** We bypass Python's slow `multiprocessing` by handling data pre-fetching in lightweight Rust threads.

---

## Quick Start: AI Training

Hexz acts as a drop-in replacement for standard PyTorch datasets.

### 1. Installation

**Note:** Hexz is currently in development (pre-release). To try it out, build from source:

```bash
# Clone the repository
git clone https://github.com/Alethic-Systems/hexz.git
cd hexz

# Build and install the Python loader
make develop
```

### 2. Stream data directly to GPU

```python
import torch
from hexz import Loader

# Connect to your compressed dataset on S3
# The index downloads in seconds; data streams on-demand.
dataset = Loader("s3://my-bucket/imagenet-21k.hxz")

# Create a standard PyTorch loader
# Hexz handles the pre-fetching and caching in Rust background threads
loader = torch.utils.data.DataLoader(dataset, batch_size=64, num_workers=4)

for batch in loader:
    # GPU is fed instantly with zero-copy overhead
    train_step(batch)

```

### 3. Pack Your Data

Use the CLI to convert your raw folder into a high-performance Hexz archive.

```bash
# Compresses, deduplicates, and indexes your dataset
hexz pack --input ./raw_images --output dataset.hxz --dedup

```

---

## System Capabilities

Hexz includes a full **Virtual Machine Manager** (`hexz-cli` + `hexz-fuse`). We maintain this to demonstrate the extreme low-latency capabilities of the file format.

### Instant VM Boot

Boot a VM directly from a compressed snapshot. The OS starts executing instructions immediately while blocks are paged in on-demand.

```bash
# Boot an Ubuntu snapshot with 4GB RAM
hexz boot ubuntu-22.04.hxz --ram 4G

```

### Live Snapshotting

Capture the exact state (Disk + RAM) of a running VM to resume later—useful for debugging training crashes or "pausing" expensive compute environments.

```bash
# Snapshot a running VM to a new file
hexz snapshot --socket /tmp/vm.sock --output checkpoint.hxz

```

---

## Architecture

The project is organized as a high-performance Rust Workspace:

| Crate | Purpose |
| --- | --- |
| **`crates/loader`** | **(Primary Product)** Python bindings via PyO3 for AI data streaming. |
| **`crates/core`** | The brain. Handles the seekable file format, compression codecs, and deduplication logic. |
| **`crates/cli`** | The `hexz` command-line tool for packing datasets and managing VMs. |
| **`crates/fuse`** | A FUSE adapter that mounts Hexz archives as local filesystems (used for VMs). |
| **`crates/server`** | High-throughput HTTP server for streaming data blocks. |

## Development

**All development commands go through the Makefile.** From the repo root, run **`make help`** to see every target. There are no separate setup scripts or one-off cargo/maturin commands to remember.

| What you want | Command |
|---------------|--------|
| List all commands | `make help` |
| One-time setup (tools + venv) | `make setup` (run `make setup-check` first to see required system packages) |
| Build CLI + Python | `make build` or `make rust` / `make develop` |
| Run all tests | `make test` |
| Lint & format | `make lint` / `make fmt` |
| Full CI locally | `make ci` |

## Documentation

* **[Quick start](docs/quickstart.md)** — Create a snapshot and read it in 5 minutes.
* **[Python API reference](docs/reference/python-api.md)** — All public APIs. Build a browsable version: **`make docs-python`** (output in `docs/_build/html`).
* **[Contributing](docs/CONTRIBUTING.md)** — Setup, branching, and PR checklist (all Makefile-based).

## Build from Source

### Prerequisites (checked by `make setup`)

Run **`make setup-check`** from the repo root. It will report any missing system packages and show install commands for your OS. You need:

* **Rust** — rustup + cargo ([rustup.rs](https://rustup.rs))
* **pkg-config** — for C library detection
* **Python 3** — for the AI loader
* **libfuse** — dev headers (VM/mount support; Linux: libfuse-dev or fuse2; macOS: macFUSE)
* **qemu** — optional, only for VM boot tests

### Compiling

From repo root (after installing any packages suggested by `make setup-check`):

```bash
make setup    # one-time: Rust components, cargo tools, Python venv
make build    # Rust workspace + Python wheel
# Or: make rust (CLI only), make develop (editable Python package)
```

## Benchmarks

* **Random Access Latency:** ~15µs per 4KB block (vs. milliseconds for standard tar/gzip).
* **Throughput:** Saturates network bandwidth before CPU limits (Zero-Copy).
* **Storage Savings:** Up to 40% reduction on standard datasets via block-level deduplication.

---

*Hexz is an open-source project exploring the limits of seekable compression and zero-copy I/O.*
