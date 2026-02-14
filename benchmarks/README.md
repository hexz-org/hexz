# Hexz Competitor Benchmarks

This directory contains benchmarks comparing Hexz against alternative data storage formats for ML workloads.

## Goal

Provide **fair, reproducible comparisons** on identical hardware using identical test data. All claims in [COMPETITIVE_COMPARISON.md](../docs/project-docs/COMPETITIVE_COMPARISON.md) must be backed by empirical measurements or cited from published sources.

## Running Benchmarks

### Quick Start

```bash
# Run all competitor benchmarks
make bench-competitors

# Run specific format
python benchmarks/competitors/webdataset_benchmark.py
python benchmarks/competitors/hdf5_benchmark.py
python benchmarks/competitors/parquet_benchmark.py
```

### Requirements

```bash
# Install all competitor libraries
pip install -r benchmarks/requirements-competitors.txt

# Or install individually:
pip install webdataset h5py pyarrow
```

## Test Data

All benchmarks use the **same test dataset** for fair comparison:

- **Dataset:** ImageNet 1K validation set (50,000 images)
- **Size:** 6.3GB raw, ~6GB compressed (varies by format)
- **Location:** `benchmarks/data/imagenet_val_50k/`

### Download Test Data

```bash
# Download ImageNet validation set
python benchmarks/download_test_data.py

# Or generate synthetic data for testing
python benchmarks/generate_synthetic_data.py --size 6.3GB
```

## Benchmark Structure

Each competitor benchmark measures:

1. **Write Performance**
   - Pack/write throughput (GB/s)
   - Compression ratio
   - Time to create dataset

2. **Read Performance**
   - Sequential read throughput (GB/s)
   - Random access latency (µs, cold and warm cache)
   - Multi-worker scaling

3. **Storage Efficiency**
   - Compressed size
   - Deduplication (if applicable)
   - Metadata overhead

4. **Integration Complexity**
   - Lines of code for PyTorch DataLoader
   - Installation complexity
   - API ergonomics (subjective, documented)

## Implemented Benchmarks

### YES Hexz (Reference)

**Status:** Complete

```bash
cargo bench --bench compression
cargo bench --bench read_throughput
cargo bench --bench write_throughput
```

**Results:** See [BENCHMARKS.md](../docs/project-docs/BENCHMARKS.md)

### WARNING WebDataset

**Status:** In progress

**Benchmark:** `competitors/webdataset_benchmark.py`

**Key metrics to validate:**
- Sequential read: Claimed ~100 MB/s, needs validation
- Random access: Claimed ~8.5 ms (shard seek), needs validation
- Write throughput: Needs measurement

**Run:**
```bash
python benchmarks/competitors/webdataset_benchmark.py
```

### WARNING HDF5

**Status:** In progress

**Benchmark:** `competitors/hdf5_benchmark.py`

**Key metrics to validate:**
- Sequential read: Claimed 1.2 GB/s, needs validation
- Random access: Claimed 345 µs, needs validation
- Compression ratio with Zstd

**Run:**
```bash
python benchmarks/competitors/hdf5_benchmark.py
```

### WARNING Parquet

**Status:** In progress (limited applicability for images)

**Benchmark:** `competitors/parquet_benchmark.py`

**Note:** Parquet is optimized for tabular data, not blobs. Benchmark included for completeness but not directly comparable.

**Run:**
```bash
python benchmarks/competitors/parquet_benchmark.py
```

### WARNING Local Files (Baseline)

**Status:** Planned

**Benchmark:** `competitors/local_files_benchmark.py`

**Purpose:** Establish baseline for raw filesystem performance (no compression).

## Output Format

All benchmarks output results in JSON for programmatic comparison:

```json
{
  "format": "webdataset",
  "version": "0.2.86",
  "test_data": "imagenet_val_50k",
  "system": {
    "cpu": "Intel i7-14700K",
    "ram": "64GB",
    "storage": "Samsung 980 Pro NVMe"
  },
  "metrics": {
    "write_throughput_gbps": 0.42,
    "sequential_read_gbps": 0.85,
    "random_access_us": 8500,
    "compressed_size_gb": 5.9,
    "compression_ratio": 1.07
  },
  "date": "2026-02-13T20:30:00Z"
}
```

## Validation Checklist

Before adding a benchmark to [COMPETITIVE_COMPARISON.md](../docs/project-docs/COMPETITIVE_COMPARISON.md):

- [ ] Benchmark uses identical test data
- [ ] Run on same hardware as Hexz benchmarks (or document differences)
- [ ] Measures same metrics (sequential, random, write)
- [ ] Code committed to `competitors/` directory
- [ ] Results reproducible by others
- [ ] Competitor library version documented
- [ ] Output includes system specs

## System Specifications

**Reference system for all benchmarks:**

- **CPU:** Intel i7-14700K (8P+12E cores, 20 cores total)
- **RAM:** 64GB DDR4
- **Storage:** Samsung 980 Pro NVMe SSD (7000 MB/s sequential read)
- **OS:** Linux 6.18.7 (Arch)
- **Kernel:** 6.18.7-arch1-1
- **Python:** 3.11.x
- **Rust:** 1.75+

**If running on different hardware:** Include specs in benchmark output and note differences in COMPETITIVE_COMPARISON.md.

## Contributing

### Adding a New Competitor Benchmark

1. Copy `template_benchmark.py` to `<format>_benchmark.py`
2. Implement the benchmark following the template structure
3. Test with: `python benchmarks/competitors/<format>_benchmark.py`
4. Verify output includes all required metrics
5. Submit PR with:
   - Benchmark code
   - Results JSON
   - Update to COMPETITIVE_COMPARISON.md

### Updating Existing Benchmarks

1. Run benchmark: `python benchmarks/competitors/<format>_benchmark.py`
2. Compare results to published claims
3. If significant difference (>10%), investigate:
   - Correct test data used?
   - Same system specs?
   - Library version match?
   - Fair configuration (e.g., same compression level)?
4. Update COMPETITIVE_COMPARISON.md with validated numbers
5. Submit PR with updated results and analysis

## Citation Policy

When citing published performance data (not benchmarked ourselves):

1. **Include source:** Link to paper, blog post, or official docs
2. **Include date:** When was the data published?
3. **Note conditions:** Hardware specs, dataset size, configuration
4. **Mark as cited:** Use [CITED: source] in comparison tables

**Example:**
```markdown
| HDF5 | Sequential read | 1.2 GB/s | [CITED: HDF Group Performance Report, 2024] |
```

## FAQ

### Why not just cite published benchmarks?

Published benchmarks often use different:
- Hardware (older CPUs, different storage)
- Datasets (synthetic vs real-world)
- Configurations (default vs optimized)

**Our policy:** Validate on identical hardware OR clearly cite source and conditions.

### How to handle format-specific optimizations?

Use **default/recommended configurations** for each format:
- WebDataset: Standard tar + gzip compression
- HDF5: Default chunking with Zstd-3
- Parquet: Snappy compression (default)

Document any non-default settings and justify why.

### What if a format doesn't support a metric?

Mark as "N/A" in comparison tables and explain why:
- tar.gz: Random access N/A (must decompress sequentially)
- Parquet: Not applicable to blob data

### Can I submit benchmarks from different hardware?

Yes, but:
1. Document exact system specs
2. Include hardware differences in comparison tables
3. Note in COMPETITIVE_COMPARISON.md: "Measured on different hardware, not directly comparable"

Prefer running on reference hardware when possible.

---

**Questions?** Open an issue or see [CONTRIBUTING.md](../docs/project-docs/CONTRIBUTING.md).
