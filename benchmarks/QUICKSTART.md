# Competitor Benchmarks Quick Start

This guide shows you how to run fair, reproducible benchmarks comparing Hexz against WebDataset, HDF5, and raw local files.

## Prerequisites

1. **Install Python dependencies:**

```bash
pip install -r benchmarks/requirements-competitors.txt
```

2. **Build Hexz:**

```bash
make rust
```

## Quick Test (Small Dataset)

For quick testing with a small dataset (1000 images, ~130MB):

```bash
cd benchmarks
./run_all_benchmarks.sh --small
```

This will:
1. Generate 1000 synthetic test images
2. Run benchmarks for local files, WebDataset, and HDF5
3. Generate comparison report

**Time:** ~2-5 minutes

## Full Benchmark (ImageNet-sized Dataset)

For realistic benchmarks with 50,000 images (~6.3GB):

```bash
cd benchmarks
./run_all_benchmarks.sh
```

**Time:** ~30-60 minutes (depending on hardware)

## Viewing Results

After benchmarks complete:

```bash
# View comparison table
cat benchmarks/results/COMPARISON.md

# View individual results
cat benchmarks/results/webdataset_results.json
cat benchmarks/results/hdf5_results.json
cat benchmarks/results/local_files_results.json
```

## Running Individual Benchmarks

You can run benchmarks individually:

### Local Files (Baseline)

```bash
python benchmarks/competitors/local_files_benchmark.py \
    --data-dir benchmarks/data/imagenet_val_50k \
    --results-file benchmarks/results/local_files_results.json
```

### WebDataset

```bash
python benchmarks/competitors/webdataset_benchmark.py \
    --data-dir benchmarks/data/imagenet_val_50k \
    --output-dir benchmarks/data/webdataset_shards \
    --num-shards 100 \
    --results-file benchmarks/results/webdataset_results.json
```

### HDF5

```bash
python benchmarks/competitors/hdf5_benchmark.py \
    --data-dir benchmarks/data/imagenet_val_50k \
    --output-file benchmarks/data/hdf5_dataset.h5 \
    --compression gzip \
    --compression-level 3 \
    --results-file benchmarks/results/hdf5_results.json
```

## Adding Hexz Results

TODO: Once Hexz Python bindings are ready, add:

```bash
python benchmarks/competitors/hexz_benchmark.py \
    --data-dir benchmarks/data/imagenet_val_50k \
    --output-file benchmarks/data/hexz_dataset.hxz \
    --compression lz4 \
    --results-file benchmarks/results/hexz_results.json
```

## Interpreting Results

The comparison report shows:

1. **Throughput Comparison**
   - Write speed (packing/creation)
   - Sequential read speed
   - Storage size

2. **Latency Comparison**
   - Cold cache (first access)
   - Warm cache (repeated access)
   - P95 latency (95th percentile)

3. **Storage Efficiency**
   - Compressed size
   - Compression ratio vs raw files

## Fair Comparison Guidelines

All benchmarks use:
- Same test data (identical images)
- Same system (hardware specs in results)
- Same metrics (throughput, latency, storage)
- Same measurement methodology

Compression settings:
- HDF5: gzip level 3 (similar to Zstd-3)
- WebDataset: default tar compression
- Hexz: LZ4 (default) or Zstd-3 (configurable)

## Troubleshooting

**Problem:** Out of memory during HDF5 benchmark

**Solution:** Reduce dataset size or increase system RAM

```bash
./run_all_benchmarks.sh --small
```

**Problem:** WebDataset benchmark slow

**Solution:** Reduce number of shards

```bash
python benchmarks/competitors/webdataset_benchmark.py --num-shards 10
```

**Problem:** Benchmark results differ from published claims

**Expected:** Results depend on hardware. Document your system specs (automatically included in results JSON).

## Next Steps

1. Run benchmarks on your hardware
2. Review `benchmarks/results/COMPARISON.md`
3. Update `docs/project-docs/COMPETITIVE_COMPARISON.md` with validated numbers
4. Submit PR with benchmark results and system specs

## Questions?

See [benchmarks/README.md](README.md) for detailed methodology and validation requirements.
