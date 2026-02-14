# Hexz Competitor Benchmarks

This directory contains comprehensive Python-based benchmarks comparing Hexz against popular alternatives for ML data loading.

## Overview

All benchmarks are designed to:
- Run in **pure Python** for apples-to-apples comparison
- Use **identical test data** across all formats
- Measure **realistic ML training scenarios**
- Show **both strengths and weaknesses** of each approach

## Requirements

```bash
# Install all benchmark dependencies
pip install -r requirements.txt
```

## Quick Start

```bash
# Generate test data (creates ~500MB of realistic image-like data)
python generate_test_data.py

# Run all benchmarks
python run_all_benchmarks.py

# Or run individual benchmarks
python benchmarks/hexz_benchmark.py
python benchmarks/webdataset_benchmark.py
python benchmarks/hdf5_benchmark.py
python benchmarks/local_files_benchmark.py
```

## Benchmark Categories

### 1. Sequential Read Throughput
Measures how fast each format can stream data sequentially (simulating first epoch).

**Metrics:**
- Throughput (MB/s)
- Total time to read entire dataset
- CPU usage

### 2. Random Access Performance
Tests shuffled access patterns (simulating training with shuffling).

**Metrics:**
- Random access latency (µs)
- Throughput with shuffled indices
- Cache effectiveness

### 3. Multi-Worker Scaling
Simulates PyTorch DataLoader with multiple workers.

**Metrics:**
- Throughput with 1, 2, 4, 8 workers
- Speedup vs single worker
- Scaling efficiency

### 4. Storage Efficiency
Compares compressed size and deduplication effectiveness.

**Metrics:**
- Storage size (compressed)
- Compression ratio
- Deduplication savings (for versioned datasets)

### 5. Write Performance
Measures how long it takes to pack/prepare datasets.

**Metrics:**
- Write throughput (MB/s)
- Time to pack 1GB of data
- CPU/memory usage

## Test Data

The `generate_test_data.py` script creates realistic ML datasets:

- **ImageNet-like**: 10,000 samples × 50KB (similar to ImageNet images)
- **Small images**: 50,000 samples × 4KB (similar to CIFAR-10)
- **Variable sizes**: Mixed sizes from 1KB to 100KB
- **Compression ratio**: ~60% (realistic for JPEG-like data)

All data is deterministically generated with controlled entropy for fair comparisons.

## Formats Tested

### Hexz
- **Strengths**: Fast random access, good compression, S3 streaming, deduplication
- **Weaknesses**: Newer format, smaller ecosystem

### WebDataset
- **Strengths**: Mature ecosystem, PyTorch integration, streaming-friendly
- **Weaknesses**: Shard-limited shuffling, no true random access

### HDF5 (h5py)
- **Strengths**: Mature, well-tested, good random access, compression
- **Weaknesses**: Slower S3 streaming, complex API

### Local Files
- **Strengths**: Simplest, no overhead, maximum compatibility
- **Weaknesses**: No compression, expensive on S3, many small files

### Parquet (for tabular data)
- **Note**: Parquet is included for completeness but is designed for tabular data, not blob storage

## Output Format

Each benchmark outputs JSON results:

```json
{
  "format": "hexz",
  "test": "sequential_read",
  "throughput_mb_s": 850.3,
  "latency_us": 6.2,
  "samples_per_sec": 17000,
  "metadata": {
    "num_samples": 10000,
    "sample_size": 4096,
    "compression": "lz4"
  }
}
```

Results are saved to `results/` directory for analysis.

## Analysis

After running benchmarks, generate comparison reports:

```bash
# Generate markdown report for docs
python analyze_results.py --output ../docs/project-docs/COMPETITIVE_COMPARISON.md

# Generate charts
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
- All formats use same compression algorithm where possible (LZ4 or equivalent)
- Same test data for all formats
- Same Python environment and dependencies
- No network I/O (local files only)
- Document any format-specific optimizations

## Contributing

When adding new benchmarks:
1. Use the `benchmark_template.py` as starting point
2. Follow the same metrics and output format
3. Document any format-specific setup requirements
4. Update this README with results

## Reproducing Published Results

To reproduce the results in `docs/project-docs/COMPETITIVE_COMPARISON.md`:

```bash
# Ensure clean environment
make clean

# Generate test data
python benchmarks/generate_test_data.py

# Run all benchmarks (takes ~10 minutes)
python benchmarks/run_all_benchmarks.py

# Generate report
python benchmarks/analyze_results.py
```

## Notes

- **WebDataset**: Requires creating shards first, which adds overhead
- **HDF5**: Best with chunked storage, chunk size affects performance
- **Hexz**: Requires building from source (see main README)
- **Local Files**: Baseline for comparison, no compression

## Validation Status

✅ **Hexz**: Results validated via `cargo bench` and Python benchmarks
🔄 **Competitors**: Results being validated through these scripts
📊 **Published Data**: Where available, cited in COMPETITIVE_COMPARISON.md

## Questions?

See main project [FAQ](../docs/FAQ.md) or open an issue.
