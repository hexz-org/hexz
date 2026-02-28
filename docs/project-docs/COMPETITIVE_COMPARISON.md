# Hexz Competitive Comparison

**Validation status:**
- Hexz block dedup: validated on real datasets (see below)
- Hexz XOR delta savings: **[UNTESTED]** — Phase 3 not yet complete; theoretical basis from Hachiuma et al. "ZipLLM" (2024)
- Storage estimates for fine-tune chains: **[UNTESTED]** — marked where they appear
- Dataset I/O benchmarks: validated on real datasets (CIFAR-10/STL-10/CIFAR-100), i7-14700K, 64GB RAM, NVMe SSD

---

## What Hexz is optimized for

- Storing many versions of the same large model checkpoint (fine-tune chains, LoRA sweeps, RLHF iterations)
- Random access to named tensors without downloading or decompressing the whole file
- Single-file portability that works over HTTP or S3 byte-range requests without infrastructure

## What Hexz is not optimized for

- Single checkpoint, no versioning (safetensors is simpler and faster to write)
- Sequential training data streaming (WebDataset/StreamingDataset have tuned DataLoader integration)
- Structured/tabular data (Parquet/DuckDB)
- Team access controls and history UI (W&B Artifacts, MLflow)
- Files under 1 GB (index overhead not worth it)

---

## Checkpoint storage comparison

Approximate storage cost for 50 fine-tunes of a 7B model (~14 GB each), where each checkpoint shares most weights with the previous.

| Approach | Storage | Notes |
|---|---|---|
| Raw `.pt`/`.safetensors` copies | ~700 GB | No dedup |
| git-lfs | ~700 GB | Tracks which blob = which commit; does not deduplicate content inside blobs |
| DVC + S3 | ~700 GB | Pointer tracker, not a content store |
| Hexz (BLAKE3 block dedup, parent chain) | **[UNTESTED]** | Identical blocks across parent/child not re-stored; depends on how much changed per fine-tune |
| Hexz (XOR delta compression) | **[UNTESTED]** | Expected to be significantly better than block dedup alone; benchmark pending Phase 3 |

The 92.4% CDC dedup rate on shifted data (validated benchmark below) applies to the case where byte-identical blocks exist. For fine-tuning where every tensor changed slightly, XOR delta (Phase 3) is needed to get significant savings.

---

## Format comparison matrix

| | Random Access | Named Tensor Load | Dedup Across Versions | XOR Delta | Single File | No Daemon | S3 Streaming |
|---|---|---|---|---|---|---|---|
| Raw files | Yes | No | No | No | No | Yes | Slow (per-file) |
| git-lfs / DVC | No | No | No | No | No | No | Yes |
| safetensors | Yes | Yes (mmap) | No | No | Yes | Yes | No |
| GGUF | Yes | Yes (mmap) | No | No | Yes | Yes | No |
| HDF5 | Yes (slow) | No | No | No | Yes | Yes | Partial |
| WebDataset | Shard-level | No | No | No | No (shards) | Yes | Yes |
| **Hexz** | **Yes** | **Yes** | **Yes (parent chain)** | **In dev (Phase 3)** | **Yes** | **Yes** | **Yes** |

---

## Validated benchmarks

### Dataset I/O (Python, real image datasets)

**System:** Intel i7-14700K, 64GB RAM, Samsung 980 Pro NVMe, Linux 6.18.7

**CIFAR-10 — 50,000 images, 108 MB:**

| Format | Sequential Read | Random Access | Shuffled Epoch | Storage |
|---|---|---|---|---|
| Local files | 387 MB/s | 6.2 µs | 360 MB/s | 107.8 MB |
| HDF5 (LZF) | 56 MB/s | 40.8 µs | 55 MB/s | 114.2 MB |
| **Hexz (LZ4)** | **1,218 MB/s** | **3.4 µs** | **525 MB/s** | 111.2 MB |

**STL-10 — 10,000 images, 28 MB:**

| Format | Sequential Read | Random Access | Shuffled Epoch | Storage |
|---|---|---|---|---|
| Local files | 502 MB/s | 5.8 µs | 471 MB/s | 27.6 MB |
| HDF5 (LZF, vlen) | 20 MB/s | 162.0 µs | 17 MB/s | 28.2 MB |
| **Hexz (LZ4)** | **1,515 MB/s** | **3.8 µs** | **826 MB/s** | 26.6 MB |

**CIFAR-100 — 20,000 images, 18 MB:**

| Format | Sequential Read | Random Access | Shuffled Epoch | Storage |
|---|---|---|---|---|
| Local files | 168 MB/s | 5.9 µs | 157 MB/s | 18.1 MB |
| HDF5 (LZF, vlen) | 7 MB/s | 151.3 µs | 6 MB/s | 18.8 MB |
| **Hexz (LZ4)** | **640 MB/s** | **3.2 µs** | **210 MB/s** | **16.1 MB** |

These benchmarks are on small datasets that fit entirely in RAM. At TB scale from S3, network bandwidth dominates. WebDataset is not benchmarked here — do not claim Hexz beats WebDataset on sequential streaming.

### Dedup efficiency (Rust benchmark)

```bash
cargo bench --bench dedup_efficiency
```

**Shifted data** — two 50 MB versions, second has 1 KB inserted at start:

| Method | Base only | Base + shifted | Dedup ratio |
|---|---|---|---|
| Fixed-size blocks | 50.2 MB | 100.4 MB | **0%** |
| CDC blocks | 50.2 MB | 54.0 MB | **92.4%** |

**Internal duplication** — 50 MB file with 25% repeated blocks:

| Method | Output | Savings |
|---|---|---|
| Fixed-size blocks | 50.2 MB | 0% |
| CDC blocks | 40.2 MB | 19.6% |

---

## When to use alternatives

| Situation | Better choice | Why |
|---|---|---|
| Single checkpoint, no versioning | safetensors | No compression overhead, universally supported, zero-copy mmap |
| Training data, sequential streaming | WebDataset / MosaicML StreamingDataset | Optimized DataLoader, mature sharding workflow |
| Structured/tabular data | Parquet, DuckDB | Column-oriented queries, broader ecosystem |
| Need `torch.load` compatibility | PyTorch native | Hexz cannot load arbitrary pickled Python objects |
| Team access controls and history UI | W&B Artifacts, MLflow | Full platforms with auth and UI |
| Dataset or model < 1 GB | Local files | Index overhead not justified |

---

## Reproducing benchmarks

```bash
# Dataset I/O benchmarks
pip install -r benchmarks/requirements.txt
python benchmarks/generate_data.py
python benchmarks/run_benchmarks.py --dataset all

# Dedup benchmarks (Rust)
cargo bench --bench dedup_efficiency

# Engine microbenchmarks
cargo bench --bench compression
cargo bench --bench sparse_access
cargo bench --bench read_throughput
```
