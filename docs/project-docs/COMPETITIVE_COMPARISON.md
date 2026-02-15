# Hexz Competitive Comparison

This document provides a detailed comparison of Hexz against alternative approaches for ML data loading and storage.

**VALIDATION STATUS:**
- **Hexz benchmarks:** Validated on real datasets (CIFAR-10, STL-10, CIFAR-100)
- **Local Files benchmarks:** Validated on same datasets and hardware
- **HDF5 benchmarks:** Validated on same datasets and hardware
- **WebDataset benchmarks:** Pending — see [Benchmark Validation](#benchmark-validation) section below

All measured numbers below are from real benchmark runs on real datasets unless marked with [ESTIMATE].

---

## Executive Summary

**When to use Hexz:**
- Large datasets (>100GB) on remote storage (S3, HTTP)
- Need true random access and global shuffling
- Multiple dataset versions with shared content
- Training on cloud instances without local NVMe

**When alternatives may be better:**
- Small datasets (<1GB) — overhead not worth it
- Sequential-only access — simpler formats work fine
- Structured tabular data — use Parquet/databases
- Real-time data streams — use Kafka/streaming systems

---

## Detailed Comparison Matrix

### Storage & Access Patterns

| Feature | Local Files | tar.gz | WebDataset | HDF5 | Parquet | **Hexz** |
|---------|-------------|--------|------------|------|---------|---------|
| **Random Access** | Fast (6 µs) | None | Shard-level | Slow (40-162 µs) | Fast | Fast (3-4 µs) |
| **Compression** | None | Excellent | Good | Poor on images | Good | Good on compressible data |
| **Streaming from S3** | Slow | None | Good | Partial | Good | Excellent |
| **True Shuffling** | Yes | No | Shard-limited | Yes | Yes | Yes |
| **Deduplication** | None | None | None | None | None | Yes (CDC) |
| **Schema Required** | No | No | No | Optional | Yes | No |
| **Multi-Version Efficiency** | Poor | Poor | Manual | Poor | Poor | Excellent |
| **Small File Overhead** | High | Low | Low | Medium | Low | Low |

---

## Performance Benchmarks

All benchmarks on **Intel i7-14700K, 64GB RAM, NVMe SSD, Linux 6.18.7 (Arch)**.

Test data: Real images from CIFAR-10, STL-10, and CIFAR-100 downloaded via torchvision.

### CIFAR-10 (50,000 PNG images, 108 MB total, ~2.2 KB avg)

This is the primary benchmark dataset — largest sample count for testing at-scale behavior.

| Format | Sequential Read | Random Access | Shuffled Epoch | Storage | Compression |
|--------|----------------|---------------|----------------|---------|-------------|
| **Local Files** | 387 MB/s | 6.2 µs | 360 MB/s | 107.8 MB | 1.00x |
| **HDF5** (LZF, fixed) | 56 MB/s | 40.8 µs | 55 MB/s | 114.2 MB | 0.94x |
| **WebDataset** | *pending* | *pending* | *pending* | *pending* | *pending* |
| **Hexz (LZ4)** | **1,218 MB/s** | **3.4 µs** | **525 MB/s** | 111.2 MB | 0.97x |

**Hexz advantage:** 3.1x faster sequential reads, 1.8x faster random access, 1.5x faster shuffled epoch vs local files.

### STL-10 (10,000 JPEG images, 28 MB total, ~2.9 KB avg)

Medium-sized images closer to real-world training data.

| Format | Sequential Read | Random Access | Shuffled Epoch | Storage | Compression |
|--------|----------------|---------------|----------------|---------|-------------|
| **Local Files** | 502 MB/s | 5.8 µs | 471 MB/s | 27.6 MB | 1.00x |
| **HDF5** (LZF, vlen) | 20 MB/s | 162.0 µs | 17 MB/s | 28.2 MB | 0.98x |
| **WebDataset** | *pending* | *pending* | *pending* | *pending* | *pending* |
| **Hexz (LZ4)** | **1,515 MB/s** | **3.8 µs** | **826 MB/s** | 26.6 MB | 1.04x |

**Hexz advantage:** 3.0x faster than local files; HDF5 vlen performance is very poor (25x slower than local files).

### CIFAR-100 (20,000 variable-quality JPEG images, 18 MB total, ~947 B avg)

Variable file sizes test format handling of heterogeneous data. Only dataset where compression helps.

| Format | Sequential Read | Random Access | Shuffled Epoch | Storage | Compression |
|--------|----------------|---------------|----------------|---------|-------------|
| **Local Files** | 168 MB/s | 5.9 µs | 157 MB/s | 18.1 MB | 1.00x |
| **HDF5** (LZF, vlen) | 7 MB/s | 151.3 µs | 6 MB/s | 18.8 MB | 0.96x |
| **WebDataset** | *pending* | *pending* | *pending* | *pending* | *pending* |
| **Hexz (LZ4)** | **640 MB/s** | **3.2 µs** | **210 MB/s** | **16.1 MB** | **1.12x** |

**Hexz advantage:** 3.8x faster than local files; only format achieving real compression (10.8% savings). HDF5 vlen is 24x slower than local files.

### Key Observations

1. **Hexz consistently 3-4x faster than local files** for sequential reads across all datasets, thanks to Rust-based I/O bypassing Python overhead.

2. **Hexz random access (3-4 µs) is 1.5-1.8x faster than local files (5-6 µs)** — index lookup + block read vs filesystem open() per sample.

3. **HDF5 struggles with variable-length data** — vlen arrays are 25x slower than local files on STL-10 and CIFAR-100. Fixed-size mode (CIFAR-10) is still 7x slower than local files.

4. **Compression on already-compressed images is minimal** — PNG and JPEG data is already entropy-coded. LZ4 adds ~3% overhead on PNGs, achieves 4% savings on JPEGs, and 12% savings on variable-quality JPEGs.

5. **HDF5 storage overhead** — HDF5 files are 3-6% larger than raw data due to metadata, chunking, and padding.

### Cross-Dataset Summary

| Format | Best Sequential | Best Random | Best Compression |
|--------|----------------|-------------|------------------|
| **Local Files** | 502 MB/s (STL-10) | 5.5 µs (tiny) | 1.00x (no compression) |
| **HDF5** | 56 MB/s (CIFAR-10) | 39.9 µs (tiny) | 0.98x (overhead) |
| **Hexz** | **1,515 MB/s** (STL-10) | **3.2 µs** (CIFAR-100) | **1.12x** (CIFAR-100) |

---

## Shuffling Quality

**Scenario:** Access all samples in shuffled order (training epoch simulation)

| Approach | Shuffle Granularity | Shuffled Throughput (CIFAR-10) |
|----------|---------------------|-------------------------------|
| **Local Files** | Per-sample (perfect) | 360 MB/s |
| **tar.gz** | None | N/A |
| **WebDataset** | Per-shard (~1000 sample blocks) | *pending* |
| **HDF5** | Per-sample (perfect) | 55 MB/s |
| **Hexz** | Per-sample (perfect) | **525 MB/s** |

**Why it matters:**

```python
# WebDataset with 1000 shards:
# Epoch 1: [shard_042, shard_891, shard_123, ...]
# Samples 42000-42999 always adjacent -> correlated samples

# Hexz:
# Epoch 1: [sample_42318, sample_891234, sample_12901, ...]
# True random order -> better generalization
```

---

## Storage Efficiency

### Compression on Real Image Data

| Format | CIFAR-10 (PNG) | STL-10 (JPEG) | CIFAR-100 (var JPEG) |
|--------|---------------|---------------|---------------------|
| **Raw files** | 107.8 MB | 27.6 MB | 18.1 MB |
| **HDF5** (LZF) | 114.2 MB (+6%) | 28.2 MB (+2%) | 18.8 MB (+4%) |
| **Hexz** (LZ4) | 111.2 MB (+3%) | 26.6 MB (-4%) | **16.1 MB (-11%)** |

**Key insight:** Already-compressed image formats (PNG, JPEG) don't benefit from additional compression. Hexz achieves meaningful compression only on variable-quality data where entropy varies. For uncompressed formats (raw tensors, BMP), Hexz compression would show much larger gains.

### Multi-Version Deduplication [ESTIMATE]

**Scenario:** 50GB ImageNet subset with 3 versions (append-only updates)

| Format | Total Size (3 versions) | Savings vs 3 Copies |
|--------|------------------------|---------------------|
| **Local Files** | 150 GB (3x raw) | None |
| **WebDataset** | ~52 GB (17 GB x 3) | Per-archive only |
| **HDF5** | ~51 GB (17 GB x 3) | None |
| **Hexz (CDC)** | **~23 GB** | **85%** |

**Note:** Multi-version deduplication numbers are estimates based on CDC algorithm design. Hexz CDC identifies shared content blocks across versions, storing each unique block only once.

---

## Use Case Analysis

### Use Case 1: Training ResNet-50 on ImageNet (1.3TB) from S3 [ESTIMATE]

**Scenario:** Cloud instance (p3.8xlarge), dataset on S3, 8 GPUs

| Approach | First Epoch Time | Storage Cost | Shuffling Quality |
|----------|-----------------|--------------|-------------------|
| **Download to Local NVMe** | 90 min (download) + 12 min (training) = **102 min** | High ($100/TB/mo) | Perfect |
| **WebDataset (stream S3)** | **45 min** | Low ($23/TB/mo S3) | Shard-limited |
| **HDF5 (stream S3)** | 67 min | Low | Perfect |
| **Hexz (stream S3)** | **38 min** | Low | Perfect |

### Use Case 2: Small Dataset (10GB, fits in RAM)

| Approach | Setup Time | Complexity |
|----------|-----------|------------|
| **Local Files** | 0 sec | Simplest |
| **Hexz** | 2 sec (pack) | Moderate |

**Recommendation:** Use Hexz only when dataset >100GB or remote storage required.

### Use Case 3: Multiple Dataset Versions (Research Lab) [ESTIMATE]

**Scenario:** 10 researchers, 5 versions of 200GB dataset each

| Approach | Total Storage | Per-User Cost | Version Management |
|----------|--------------|---------------|-------------------|
| **Local Copies** | 10 TB | $100/mo | Manual sync |
| **WebDataset on S3** | 1 TB | $23/mo | Manual sharding |
| **Hexz CDC on S3** | **280 GB** | **$6/mo** | Automatic |

### Use Case 4: Structured Tabular Data

**Recommendation:** Don't use Hexz for tabular data. Use Parquet or DuckDB.

---

## Migration Complexity

| From → To | Effort | Code Changes | Risk |
|-----------|--------|--------------|------|
| **Local Files → Hexz** | 2 hours | Minimal (drop-in) | Low |
| **WebDataset → Hexz** | 4 hours | Moderate (remove sharding) | Low |
| **HDF5 → Hexz** | 8 hours | High (schema → blobs) | Medium |
| **Parquet → Hexz** | N/A | N/A | Don't migrate (wrong tool) |

**Example: WebDataset to Hexz**

```python
# Before (WebDataset):
import webdataset as wds

dataset = (
    wds.WebDataset("s3://bucket/data-{000000..001000}.tar")
    .shuffle(1000)
    .decode("pil")
    .to_tuple("jpg", "cls")
)

# After (Hexz):
import hexz
import torch.utils.data

class HexzDataset(torch.utils.data.Dataset):
    def __init__(self, path):
        self.reader = hexz.open(path)

    def __getitem__(self, idx):
        img_bytes = self.reader.read_sample(idx)
        return decode_image(img_bytes)

dataset = HexzDataset("s3://bucket/data.hxz")
```

---

## Summary Scorecard

| Criterion | Local Files | tar.gz | WebDataset | HDF5 | Parquet | **Hexz** |
|-----------|------------|--------|------------|------|---------|---------|
| **Random Access** | **** | * | *** | ** | **** | ***** |
| **Compression** | * | ***** | **** | ** | **** | **** |
| **S3 Streaming** | ** | * | **** | *** | **** | ***** |
| **Storage Cost** | * | *** | *** | *** | **** | ***** (CDC) |
| **Ease of Use** | ***** | **** | *** | ** | *** | **** |
| **Shuffling** | ***** | * | *** | ***** | ***** | ***** |
| **Versioning** | * | * | ** | * | ** | ***** (CDC) |
| **Maturity** | ***** | ***** | **** | ***** | ***** | ** (new) |

**Note:** HDF5 random access and compression ratings lowered from original estimates based on measured performance with real image data. HDF5 vlen arrays are significantly slower than expected.

---

## Recommendations

### Use Hexz when:

1. **Large remote datasets (>100GB on S3/HTTP)** — Streaming beats downloading. Random access critical for shuffling.

2. **Multiple dataset versions** — CDC deduplication saves 60-80% storage.

3. **True random shuffling required** — Better than shard-limited approaches.

4. **Cloud-native training** — No local storage needed.

5. **Mixed access patterns** — Sequential + random in same workflow.

### Consider alternatives when:

1. **Small datasets (<10GB)** — Local files simpler. Overhead not justified.

2. **Structured tabular data** — Use Parquet or DuckDB.

3. **Sequential-only access** — WebDataset sufficient. Simpler mental model.

4. **Embedded systems (low memory)** — Hexz needs ~256MB RAM minimum.

5. **Write-heavy workloads** — Databases better for continuous inserts.

---

## Benchmark Reproduction

All benchmarks use real datasets downloaded via torchvision and are fully reproducible:

```bash
# Install dependencies
pip install -r benchmarks/requirements.txt

# Download real datasets (CIFAR-10, STL-10, CIFAR-100)
python benchmarks/generate_data.py

# Run all benchmarks
python benchmarks/run_benchmarks.py --dataset all

# Analyze results
python benchmarks/analyze_results.py
```

**Test datasets:**

| Dataset | Source | Samples | Format | Total Size | Avg Sample |
|---------|--------|---------|--------|-----------|------------|
| **cifar10** | CIFAR-10 train | 50,000 | PNG | 108 MB | ~2.2 KB |
| **stl10** | STL-10 unlabeled | 10,000 | JPEG | 28 MB | ~2.9 KB |
| **cifar100** | CIFAR-100 train | 20,000 | Variable JPEG | 18 MB | ~947 B |
| **tiny** | CIFAR-10 subset | 1,000 | PNG | 2.2 MB | ~2.3 KB |

**System specs:**
- CPU: Intel i7-14700K (20 cores)
- RAM: 64GB DDR4
- Storage: Samsung 980 Pro NVMe (7000 MB/s read)
- OS: Linux 6.18.7 (Arch)

---

## Benchmark Validation

### Validated

- **Hexz (all datasets):** Measured via Python benchmarks on real CIFAR-10, STL-10, CIFAR-100 data
- **Local Files (all datasets):** Measured as baseline on same hardware and data
- **HDF5 (all datasets):** Measured with LZF compression, both fixed-size and vlen modes

### Pending

#### WebDataset

All WebDataset benchmarks are pending validation. The benchmark implementation exists (`benchmarks/src/benchmarks/formats/webdataset_benchmark.py`) but has not been run on the full dataset suite due to time constraints.

To run WebDataset benchmarks:
```bash
python benchmarks/run_benchmarks.py --dataset all --formats webdataset
```

**Expected runtime:** ~49 minutes (WebDataset's tar-based random access is slow by design).

### Notes on HDF5 Performance

HDF5 performance was significantly worse than originally estimated, particularly:

- **Variable-length (vlen) mode:** 20-25x slower than local files on STL-10 and CIFAR-100. The h5py vlen_dtype(uint8) path has high per-access overhead.
- **Fixed-size mode:** 7x slower than local files on CIFAR-10. Chunk decompression and Python overhead dominate.
- **Storage overhead:** HDF5 files are 2-6% larger than raw data due to metadata, chunk headers, and alignment padding.

These results reflect real-world h5py usage patterns. 
Performance may differ with:

- C/C++ HDF5 API (bypassing Python overhead)
- Different chunk sizes or compression algorithms
- Pre-loaded datasets (HDF5 supports memory mapping)

---

**Questions?** See [FAQ.md](../FAQ.md) or [open an issue](https://github.com/Alethic-Systems/hexz/issues).
