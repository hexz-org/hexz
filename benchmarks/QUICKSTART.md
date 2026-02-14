# Benchmark Quickstart Guide

This guide will help you run comprehensive benchmarks comparing Hexz against competitors.

## Prerequisites

1. **Build Hexz from source**:
   ```bash
   cd /home/will/projects/hexz
   make setup
   make develop  # Installs Python package
   make rust     # Builds CLI tool
   ```

2. **Install benchmark dependencies**:
   ```bash
   cd benchmarks
   pip install -r requirements.txt
   ```

## Quick Run (5 minutes)

For a quick test using a tiny dataset:

```bash
# 1. Generate tiny test data (~4MB)
python generate_test_data.py --quick

# 2. Run all benchmarks on tiny dataset
python run_all_benchmarks.py --dataset tiny

# 3. View results
python analyze_results.py
```

## Full Benchmarks (30 minutes)

For comprehensive benchmarks with realistic dataset sizes:

```bash
# 1. Generate all test datasets (~500MB)
python generate_test_data.py

# 2. Run benchmarks on different datasets
python run_all_benchmarks.py --dataset cifar_like    # 50K samples × 4KB
python run_all_benchmarks.py --dataset imagenet_like # 10K samples × 50KB

# 3. Generate markdown report
python analyze_results.py --output benchmark_report.md
```

## Understanding the Output

Each benchmark produces JSON results in `results/` directory:

```json
{
  "format_name": "hexz",
  "test_name": "sequential_read",
  "throughput_mb_s": 850.3,
  "latency_us": 6.2,
  "samples_per_sec": 17000
}
```

### Key Metrics

- **throughput_mb_s**: MB per second (higher is better)
- **latency_us**: Microseconds per sample (lower is better)
- **samples_per_sec**: Samples per second (higher is better)

### Test Types

1. **sequential_read**: Simulates first training epoch (no shuffling)
2. **random_read**: Tests random access performance
3. **shuffled_epoch**: Simulates training with full shuffling

## Interpreting Results

### What Hexz Should Excel At

✅ **Random access latency**: Should be <10µs (vs 100µs+ for others)
✅ **Shuffled epoch throughput**: Should match or exceed sequential
✅ **Storage efficiency**: With deduplication enabled

### Areas Where Competitors May Lead

- **Sequential reads on local files**: Baseline has no overhead
- **Write performance**: HDF5 may be faster for initial packing
- **Ecosystem maturity**: WebDataset has more PyTorch integration

## Skipping Formats

If you don't have certain dependencies installed:

```bash
# Skip formats that aren't installed
python run_all_benchmarks.py --skip webdataset hdf5

# Just benchmark hexz vs local files (no extra deps)
python run_all_benchmarks.py --skip webdataset hdf5
```

## Troubleshooting

### "hexz module not found"

```bash
# Make sure you built and installed hexz
cd /home/will/projects/hexz
make develop
```

### "hexz CLI not found"

```bash
# Build the CLI tool
cd /home/will/projects/hexz
make rust

# Add to PATH or use absolute path
export PATH="$PATH:/home/will/projects/hexz/target/release"
```

### "webdataset not installed"

```bash
pip install webdataset
```

### "h5py not installed"

```bash
pip install h5py
```

## Advanced Usage

### Custom Dataset

Create your own test dataset:

```python
from generate_test_data import TestDataGenerator
from pathlib import Path

generator = TestDataGenerator(Path("./data"))
dataset = generator.generate_dataset(
    name="custom",
    num_samples=100000,
    sample_size=8192,
    image_like=True
)
```

### Benchmark Individual Format

```bash
# Just test hexz
python hexz_benchmark.py

# Just test HDF5
python hdf5_benchmark.py
```

### Clear Old Results

```bash
rm -rf results/*.json
rm -rf data/  # Warning: removes all test data
```

## Next Steps

After running benchmarks:

1. Review results: `cat results/*.json`
2. Generate report: `python analyze_results.py --output report.md`
3. Update docs: Copy results to `../docs/project-docs/COMPETITIVE_COMPARISON.md`
4. Share findings: Include in PRs or issues

## Expected Performance (Reference System)

On Intel i7-14700K with NVMe SSD:

| Format | Sequential Read | Random Access | Shuffled Epoch |
|--------|----------------|---------------|----------------|
| Hexz | ~850 MB/s | ~6 µs | ~800 MB/s |
| HDF5 | ~600 MB/s | ~150 µs | ~550 MB/s |
| WebDataset | ~400 MB/s | ~8 ms* | ~400 MB/s |
| Local Files | ~1200 MB/s | ~120 µs | ~1000 MB/s |

\*WebDataset random access is shard-limited, not true random access

## Questions?

See main [README.md](README.md) or open an issue at:
https://github.com/Alethic-Systems/hexz/issues
