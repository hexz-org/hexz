# Hexz Competitive Comparison

**Validation status for all numbers in this document:**
- Hexz: Validated on real datasets (CIFAR-10, STL-10, CIFAR-100) on Intel i7-14700K, 64GB RAM, NVMe SSD, Linux
- Local Files: Validated as baseline on same hardware
- HDF5: Validated on same hardware and datasets
- WebDataset: **Not benchmarked.** Rows are omitted rather than estimated.
- Multi-version dedup estimates: Marked `[ESTIMATE]` — based on validated single-file dedup benchmarks, extrapolated to multi-file scenarios

---

## What Hexz is optimized for

- Many versions of the same large binary file (checkpoints, dataset iterations)
- Random access to specific parts of a large compressed archive
- Single-file portability over HTTP or S3 without full download

## What Hexz is not optimized for

- Maximum sequential streaming throughput (WebDataset/StreamingDataset have tuned DataLoader integration)
- Structured tabular data (use Parquet or DuckDB)
- Datasets under 1GB (overhead not worth it)
- Write-heavy workloads

---

## Validated performance benchmarks

**System:** Intel i7-14700K, 64GB RAM, Samsung 980 Pro NVMe, Linux 6.18.7

**Datasets:** Real images downloaded via torchvision. Not synthetic.

### CIFAR-10 — 50,000 PNG images, 108MB total

| Format | Sequential Read | Random Access | Shuffled Epoch | Storage |
|---|---|---|---|---|
| Local Files | 387 MB/s | 6.2 µs | 360 MB/s | 107.8 MB |
| HDF5 (LZF, fixed) | 56 MB/s | 40.8 µs | 55 MB/s | 114.2 MB |
| **Hexz (LZ4)** | **1,218 MB/s** | **3.4 µs** | **525 MB/s** | 111.2 MB |

### STL-10 — 10,000 JPEG images, 28MB total

| Format | Sequential Read | Random Access | Shuffled Epoch | Storage |
|---|---|---|---|---|
| Local Files | 502 MB/s | 5.8 µs | 471 MB/s | 27.6 MB |
| HDF5 (LZF, vlen) | 20 MB/s | 162.0 µs | 17 MB/s | 28.2 MB |
| **Hexz (LZ4)** | **1,515 MB/s** | **3.8 µs** | **826 MB/s** | 26.6 MB |

### CIFAR-100 — 20,000 variable-quality JPEG images, 18MB total

| Format | Sequential Read | Random Access | Shuffled Epoch | Storage |
|---|---|---|---|---|
| Local Files | 168 MB/s | 5.9 µs | 157 MB/s | 18.1 MB |
| HDF5 (LZF, vlen) | 7 MB/s | 151.3 µs | 6 MB/s | 18.8 MB |
| **Hexz (LZ4)** | **640 MB/s** | **3.2 µs** | **210 MB/s** | **16.1 MB** |

### Observations

1. Hexz is 3-4× faster than local files on sequential reads across all tested datasets. The gap is Python overhead — Hexz reads in Rust and transfers via the buffer protocol, bypassing most Python I/O machinery.
2. Hexz random access (3-4 µs) is faster than local files (5-6 µs) because it avoids a `open()` syscall per sample.
3. HDF5 with variable-length arrays (vlen mode) is 20-25× slower than local files. This is a known h5py overhead issue, not a fundamental HDF5 limitation.
4. Compression on already-compressed images (PNG, JPEG) is minimal. Hexz achieves meaningful compression only on CIFAR-100's variable-quality JPEGs (-11%). For raw tensor data (float32 weights) compression would be significantly larger.
5. These benchmarks are on datasets that fit entirely in RAM on a fast machine. At TB scale from S3 the network is the bottleneck. These numbers do not predict S3 performance.

---

## Deduplication benchmarks (validated)

**Benchmark:** `cargo bench --bench dedup_efficiency`

### Shifted data (the realistic versioning scenario)

Two 50MB versions of the same data, second version has 1KB inserted at start. Both packed into one archive with a shared dedup map.

| Method | Base only | Base + shifted | Dedup of shifted |
|---|---|---|---|
| Fixed-size blocks | 50.2 MB | 100.4 MB | **0%** |
| CDC blocks | 50.2 MB | 54.0 MB | **92.4%** |

Fixed-size block dedup produces zero savings when data shifts. This is the boundary shift problem — inserting bytes shifts every subsequent block boundary, making all downstream blocks appear new. CDC computes boundaries from content, so they re-sync a few blocks after the insertion.

### 25% internal duplication

50MB file with 25% repeated blocks.

| Method | Output size | Space savings |
|---|---|---|
| Fixed-size blocks | 50.2 MB | 0% |
| CDC blocks | 40.2 MB | 19.6% |

---

## Checkpoint storage comparison

Approximate storage cost for 50 fine-tuned checkpoints of a 7B model (~14GB each), where each checkpoint shares ~95% of content with the previous one.

| Approach | Storage cost | Notes |
|---|---|---|
| Raw file copies | ~700 GB | No dedup |
| git-lfs | ~700 GB | Tracks which blob = which version, does not deduplicate content |
| DVC + S3 | ~700 GB | Same as git-lfs — pointer tracker, not a content store |
| Hexz thin snapshots + CDC | **[ESTIMATE] ~35-60 GB** | Base copy + deltas. Actual savings depend on how much the weights change per run. |

The Hexz estimate is based on the validated 92.4% dedup rate on shifted data. Real checkpoint savings will vary — LoRA checkpoints that only modify adapter weights will save more; full fine-tunes that modify all layers will save less.

**Caveat:** Cross-file dedup via a shared external index is not yet implemented. The thin snapshot chain (`--parent`) is the current mechanism. It works for a linear chain (v1 → v2 → v3) but not for arbitrary graph relationships between checkpoints.

---

## Format comparison matrix

| | Random Access | Dedup Across Versions | Single File | No Daemon | S3 Streaming |
|---|---|---|---|---|---|
| Raw files | Yes | No | No | Yes | Slow |
| tar.gz | No | No | Yes | Yes | No |
| HDF5 | Yes (slow in Python) | No | Yes | Yes | Partial |
| safetensors | Yes | No | Yes | Yes | No |
| WebDataset | Shard-level only | No | No (shards) | Yes | Yes |
| git-lfs / DVC | No | No | No | No | Yes |
| Zarr | Yes | No | No (directory) | Yes | Yes |
| **Hexz** | **Yes** | **Yes (thin snapshots + CDC)** | **Yes** | **Yes** | **Yes** |

---

## When to use alternatives

| Situation | Better choice | Why |
|---|---|---|
| Single checkpoint, no versioning | safetensors | No compression overhead, universally supported, zero-copy mmap |
| Training data, sequential streaming only | WebDataset | Optimized DataLoader integration, simpler sharding workflow |
| Structured/tabular data | Parquet, DuckDB | Column-oriented queries, broader ecosystem |
| Need full `torch.load` compatibility | PyTorch native | Hexz cannot load arbitrary pickled Python objects |
| Dataset < 1GB | Local files | Overhead not justified |
| Need team access controls and a UI | W&B Artifacts, MLflow | Full platforms with authentication and history UI |

---

## Reproducing benchmarks

```bash
# Download real test datasets
pip install -r benchmarks/requirements.txt
python benchmarks/generate_data.py

# Run format benchmarks
python benchmarks/run_benchmarks.py --dataset all

# Run dedup benchmarks (Rust)
cargo bench --bench dedup_efficiency

# Run engine microbenchmarks
cargo bench --bench compression
cargo bench --bench sparse_access
cargo bench --bench read_throughput
```
