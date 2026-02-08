
# Strata: High-Performance Data Streaming Engine

**Strata** is a Rust-based streaming engine designed to eliminate "GPU Starvation" in AI training. It allows PyTorch to stream massive datasets directly from compressed S3 storage into GPU memory, bypassing the Python GIL and OS page cache.

At its core, Strata is a **seekable, deduplicated compression filesystem**.

> **Why is there VM code in here?**
> Strata’s engine is so low-latency that it was originally built to **boot entire operating systems** over the network in milliseconds. This has been adapted to load large datasets for machine learning.

## The Problem: The "Data Bottleneck"

Modern GPUs are too fast for standard data loaders.

1. **S3 is high-latency:** Fetching millions of small images one-by-one is slow.
2. **Existing formats are rigid:** Tools like `WebDataset` require "sharding" data into thousands of tar files, breaking random shuffling.
3. **Storage is expensive:** Redundant data (checkpoints, code, synthetic data) wastes PB of storage.

## The Solution: Strata

Strata introduces a **Seekable Compressed Archive**.

* **Stream, Don't Shard:** Store your 10TB dataset as a single compressed stream. Strata can seek to *any* individual sample ID instantly without decompressing the whole block.
* **Native Deduplication:** Identical blocks are stored once. A dataset with 50% redundancy uses 50% less storage and bandwidth automatically.
* **Rust-Powered Concurrency:** We bypass Python's slow `multiprocessing` by handling data pre-fetching in lightweight Rust threads.

---

## Quick Start: AI Training

Strata acts as a drop-in replacement for standard PyTorch datasets.

### 1. Installation

**Note:** Strata is currently in development (pre-release). To try it out, build from source:

```bash
# Clone the repository
git clone https://github.com/willmccallion/strata.git
cd strata

# Build the Python loader
maturin develop --manifest-path crates/loader/Cargo.toml --release
```

### 2. Stream data directly to GPU

```python
import torch
from strata import StrataLoader

# Connect to your compressed dataset on S3
# The index downloads in seconds; data streams on-demand.
dataset = StrataLoader("s3://my-bucket/imagenet-21k.st")

# Create a standard PyTorch loader
# Strata handles the pre-fetching and caching in Rust background threads
loader = torch.utils.data.DataLoader(dataset, batch_size=64, num_workers=4)

for batch in loader:
    # GPU is fed instantly with zero-copy overhead
    train_step(batch)

```

### 3. Pack Your Data

Use the CLI to convert your raw folder into a high-performance Strata archive.

```bash
# Compresses, deduplicates, and indexes your dataset
strata pack --input ./raw_images --output dataset.st --dedup

```

---

## System Capabilities

Strata includes a full **Virtual Machine Manager** (`strata-cli` + `strata-fuse`). We maintain this to demonstrate the extreme low-latency capabilities of the file format.

### Instant VM Boot

Boot a VM directly from a compressed snapshot. The OS starts executing instructions immediately while blocks are paged in on-demand.

```bash
# Boot an Ubuntu snapshot with 4GB RAM
strata boot ubuntu-22.04.st --ram 4G

```

### Live Snapshotting

Capture the exact state (Disk + RAM) of a running VM to resume later—useful for debugging training crashes or "pausing" expensive compute environments.

```bash
# Snapshot a running VM to a new file
strata snapshot --socket /tmp/vm.sock --output checkpoint.st

```

---

## Architecture

The project is organized as a high-performance Rust Workspace:

| Crate | Purpose |
| --- | --- |
| **`crates/loader`** | **(Primary Product)** Python bindings via PyO3 for AI data streaming. |
| **`crates/core`** | The brain. Handles the seekable file format, compression codecs, and deduplication logic. |
| **`crates/cli`** | The `strata` command-line tool for packing datasets and managing VMs. |
| **`crates/fuse`** | A FUSE adapter that mounts Strata archives as local filesystems (used for VMs). |
| **`crates/server`** | High-throughput HTTP server for streaming data blocks. |

## Build from Source

### Prerequisites

* Rust (stable)
* `maturin` (for Python integration)
* `libfuse` headers (only if building VM support)
* `qemu` (only if running VMs)

### Compiling

```bash
# 1. Build the AI Loader (Python Wheel)
maturin build --release --manifest-path crates/loader/Cargo.toml

# 2. Build the CLI Tool (Packer & VM Manager)
cargo build --release --bin strata

```

## Benchmarks

* **Random Access Latency:** ~15µs per 4KB block (vs. milliseconds for standard tar/gzip).
* **Throughput:** Saturates network bandwidth before CPU limits (Zero-Copy).
* **Storage Savings:** Up to 40% reduction on standard datasets via block-level deduplication.

---

*Strata is an open-source project exploring the limits of seekable compression and zero-copy I/O.*
