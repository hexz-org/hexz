# Hexz Competitor Benchmarks

Comprehensive Python-based benchmarks comparing Hexz against popular alternatives for ML data loading.

## Overview

All benchmarks are designed to:
- Run in **pure Python** for apples-to-apples comparison
- Use **identical test data** across all formats
- Measure **realistic ML training scenarios**
- Show **both strengths and weaknesses** of each approach

## Requirements

```bash
pip install -r requirements.txt
```

## Quick Start

```bash
# Generate tiny test data (~4MB) for a quick run (~5 min)
python generate_test_data.py --quick

# Run all benchmarks on tiny dataset
python run_benchmarks.py --quick

# View results
python analyze_results.py
```

## Full Benchmarks (~30 min)

```bash
# Generate all test datasets (~500MB)
python generate_test_data.py

# Run benchmarks on different datasets
python run_benchmarks.py --dataset cifar_like    # 50K samples x 4KB
python run_benchmarks.py --dataset imagenet_like # 10K samples x 50KB

# Generate markdown report
python analyze_results.py --output benchmark_report.md
```

## Benchmark Categories

### 1. Sequential Read Throughput
Measures how fast each format can stream data sequentially (simulating first epoch).

### 2. Random Access Performance
Tests shuffled access patterns (simulating training with shuffling).

### 3. Multi-Worker Scaling
Simulates PyTorch DataLoader with multiple workers (1, 2, 4, 8).

### 4. Storage Efficiency
Compares compressed size and deduplication effectiveness.

### 5. Write Performance
Measures how long it takes to pack/prepare datasets.

## Formats Tested

| Format | Strengths | Weaknesses |
|--------|-----------|------------|
| **Hexz** | Fast random access, good compression, S3 streaming, dedup | Newer, smaller ecosystem |
| **WebDataset** | Mature ecosystem, PyTorch integration, streaming | Shard-limited shuffling, no true random access |
| **HDF5** | Mature, well-tested, good random access | Slower S3 streaming, complex API |
| **Local Files** | Simplest, no overhead, maximum compatibility | No compression, expensive on S3 |

## Test Data

The `generate_test_data.py` script creates realistic ML datasets:

- **ImageNet-like**: 10,000 samples x 50KB
- **Small images**: 50,000 samples x 4KB (CIFAR-like)
- **Variable sizes**: Mixed sizes from 1KB to 100KB

All data is deterministically generated with controlled entropy.

## Output Format

Each benchmark outputs JSON results to the `results/` directory:

```json
{
  "format": "hexz",
  "test": "sequential_read",
  "throughput_mb_s": 850.3,
  "latency_us": 6.2,
  "samples_per_sec": 17000
}
```

### Key Metrics

- **throughput_mb_s**: MB per second (higher is better)
- **latency_us**: Microseconds per sample (lower is better)
- **samples_per_sec**: Samples per second (higher is better)

## Analysis

After running benchmarks, generate comparison reports:

```bash
python analyze_results.py --output ../docs/project-docs/COMPETITIVE_COMPARISON.md
python analyze_results.py --charts
```

## Benchmark Methodology

### Hardware Specs
All benchmarks should be run on identical hardware. Current reference system:
- **CPU**: Intel i7-14700K (20 cores)
- **RAM**: 64GB DDR4
- **Storage**: NVMe SSD
- **OS**: Linux 6.18.7

### Test Procedure
1. **Warm up**: Run each benchmark once to warm up caches
2. **Multiple runs**: Each benchmark runs 5 times, median reported
3. **Cold cache**: Drop caches between runs where applicable
4. **Isolation**: Stop unnecessary services, pin CPU frequencies

### Fair Comparison Rules
- All formats use same compression algorithm where possible (LZ4)
- Same test data for all formats
- Same Python environment and dependencies
- No network I/O (local files only)

## Expected Performance (Reference System)

On Intel i7-14700K with NVMe SSD:

| Format | Sequential Read | Random Access | Shuffled Epoch |
|--------|----------------|---------------|----------------|
| Hexz | ~850 MB/s | ~6 us | ~800 MB/s |
| HDF5 | ~600 MB/s | ~150 us | ~550 MB/s |
| WebDataset | ~400 MB/s | ~8 ms* | ~400 MB/s |
| Local Files | ~1200 MB/s | ~120 us | ~1000 MB/s |

\*WebDataset random access is shard-limited, not true random access.

## Troubleshooting

- **"hexz module not found"**: Run `make develop` from repo root.
- **"hexz CLI not found"**: Run `make rust` from repo root, then add `target/release` to `PATH`.
- **"webdataset/h5py not installed"**: Run `pip install -r requirements.txt`.

## Reproducing Published Results

```bash
make clean
python benchmarks/generate_test_data.py
python benchmarks/run_benchmarks.py
python benchmarks/analyze_results.py
```
