# AI/ML Benchmark Quick Start

From the **repository root**, use the **Makefile** for the main benchmark entry point: **`make bench`**. The commands below are **AI-specific** `cargo bench` invocations for this harness; run them from the **repo root**.

## Overview

The AI benchmark suite contains 6 comprehensive benchmark modules designed to validate Hexz's performance for machine learning workloads. All benchmarks are production-ready and follow Criterion.rs best practices.

## Running Benchmarks

### Run All AI Benchmarks

```bash
cargo bench --bench "ai_*"
```

### Run Individual Benchmarks

```bash
# Data loader patterns
cargo bench --bench ai_dataloader

# Shuffling performance
cargo bench --bench ai_shuffle

# Prefetching strategies
cargo bench --bench ai_prefetch

# Multi-worker scaling
cargo bench --bench ai_multiworker

# Tensor operations
cargo bench --bench ai_tensor_ops

# End-to-end ML workloads
cargo bench --bench ai_ml_workloads
```

### Run Specific Tests

```bash
# Run only sequential access tests
cargo bench --bench ai_dataloader -- "Sequential"

# Run only 8-worker tests
cargo bench --bench ai_multiworker -- "workers/8"

# Run only shuffle scaling tests
cargo bench --bench ai_shuffle -- "Scaling"
```

### Quick Validation (Test Mode)

Test that benchmarks work without full measurement:

```bash
cargo bench --bench ai_dataloader -- --test
cargo bench --bench ai_shuffle -- --test
cargo bench --bench ai_prefetch -- --test
cargo bench --bench ai_multiworker -- --test
cargo bench --bench ai_tensor_ops -- --test
cargo bench --bench ai_ml_workloads -- --test
```

## Benchmark Categories

### 1. Data Loader (`ai_dataloader`)

**Purpose**: Simulates PyTorch/TensorFlow DataLoader behavior

**Tests**:
- `Sequential`: Sequential iteration through samples
- `RandomAccess`: Shuffled access patterns
- `Batching`: Batch loading with sizes 1-128
- `SampleSize`: Different tensor sizes (1KB to 1MB)
- `CacheWarmup`: Cold vs. warm cache performance

**Example Output**:
```
DataLoader/Sequential/samples/10000
                        time:   [45.2 ms 45.8 ms 46.4 ms]
                        thrpt:  [861 MB/s 872 MB/s 883 MB/s]
```

### 2. Shuffling (`ai_shuffle`)

**Purpose**: Measures Fisher-Yates shuffling performance at scale

**Tests**:
- `Scaling`: 1K to 10M samples
- `Components`: Allocation vs. permutation breakdown
- `Determinism`: Verification overhead
- `PRNGComparison`: Xorshift64 vs. alternatives
- `AccessPattern`: Cache impact of shuffle

**Example Output**:
```
Shuffle/Scaling/1000000
                        time:   [18.2 ms 18.5 ms 18.8 ms]
                        thrpt:  [53.2 Melem/s 54.1 Melem/s 54.9 Melem/s]
```

### 3. Prefetching (`ai_prefetch`)

**Purpose**: Evaluates prefetching strategies for hiding I/O latency

**Tests**:
- `WindowSize`: Windows from 0 to 32 blocks
- `StridedAccess`: Non-sequential patterns
- `Adaptive`: Adaptive vs. fixed prefetching
- `BlockSize`: Different block sizes
- `HitRate`: Cache hit rate simulation

**Example Output**:
```
Prefetch/WindowSize/blocks/8
                        time:   [125 ms 128 ms 131 ms]
                        thrpt:  [489 MB/s 500 MB/s 512 MB/s]
```

### 4. Multi-Worker (`ai_multiworker`)

**Purpose**: Tests parallel data loading (PyTorch num_workers)

**Tests**:
- `Scaling`: 1-16 workers
- `Contention`: Overlapping vs. partitioned access
- `RoundRobin`: Round-robin distribution
- `LoadBalance`: Imbalanced workloads
- `Lifecycle`: Worker spawn/join overhead

**Example Output**:
```
MultiWorker/Scaling/workers/8
                        time:   [65.2 ms 66.1 ms 67.0 ms]
                        thrpt:  [596 MB/s 604 MB/s 612 MB/s]
                        speedup: 7.2x vs single worker
```

### 5. Tensor Operations (`ai_tensor_ops`)

**Purpose**: Measures tensor-specific operation overhead

**Tests**:
- `SizeScaling`: MNIST, CIFAR, ImageNet sizes
- `ZeroCopy`: Zero-copy vs. memcpy
- `Batching`: Batched tensor loading
- `Preprocessing`: Reshape, normalize, transpose
- `Alignment`: SIMD alignment overhead
- `Concatenation`: Tensor concatenation

**Example Output**:
```
TensorOps/SizeScaling/type/ImageNet224
                        time:   [8.2 ms 8.4 ms 8.6 ms]
                        thrpt:  [1.75 GB/s 1.80 GB/s 1.84 GB/s]
```

### 6. ML Workloads (`ai_ml_workloads`)

**Purpose**: End-to-end training scenarios

**Tests**:
- `MultiEpoch`: Multi-epoch training with shuffling
- `TrainValSplit`: Training + validation phases
- `BatchSize`: Batch size scaling (1-512)
- `CheckpointResume`: Resume from checkpoints
- `Augmentation`: Data augmentation pipelines
- `DropLast`: Drop incomplete batches
- `SubsetSampling`: Random subset sampling

**Example Output**:
```
MLWorkload/MultiEpoch/epochs/5
                        time:   [2.45 s 2.48 s 2.51 s]
                        thrpt:  [79.7 MB/s 80.6 MB/s 81.6 MB/s]
```

## Interpreting Results

### Key Metrics

1. **Throughput (MB/s)**: Data transfer rate
   - Good: >500 MB/s for sequential
   - Acceptable: >200 MB/s for random access

2. **Latency (ms)**: Time per operation
   - Good: <10ms per batch
   - Acceptable: <50ms per batch

3. **Speedup**: Parallel vs. sequential
   - Good: >7x with 8 workers
   - Acceptable: >4x with 8 workers

### Baseline Comparisons

```bash
# Establish baseline
cargo bench --bench "ai_*" -- --save-baseline current

# After changes, compare
cargo bench --bench "ai_*" -- --baseline current

# View detailed comparison
open target/criterion/report/index.html
```

## Performance Targets

| Benchmark | Metric | Target | Excellent |
|-----------|--------|--------|-----------|
| Sequential throughput | MB/s | >200 | >500 |
| Random access | MB/s | >100 | >300 |
| Shuffle 1M samples | ms | <50 | <20 |
| 8-worker scaling | speedup | >4x | >7x |
| Cache hit rate (epoch 2) | % | >80% | >95% |

## Common Use Cases

### Find Optimal Batch Size

```bash
cargo bench --bench ai_ml_workloads -- "BatchSize"
```

### Find Optimal Worker Count

```bash
cargo bench --bench ai_multiworker -- "Scaling"
```

### Find Optimal Prefetch Window

```bash
cargo bench --bench ai_prefetch -- "WindowSize"
```

### Profile with Flamegraph

```bash
cargo install flamegraph
cargo flamegraph --bench ai_dataloader -- --bench "Sequential/10000"
```

### Generate HTML Reports

Criterion automatically generates detailed HTML reports:

```bash
cargo bench --bench ai_dataloader
open target/criterion/DataLoader/report/index.html
```

## Troubleshooting

### Benchmarks Too Slow

Reduce sample counts in benchmark code:

```rust
// In bench functions, reduce these values:
let num_samples = 100;  // Instead of 10000
let num_epochs = 1;     // Instead of 10
```

### Out of Memory

Reduce dataset sizes:

```rust
let sample_size = 1024;  // Instead of 1048576
let num_workers = 4;     // Instead of 16
```

### Inconsistent Results

Ensure system is idle:

```bash
# Close other applications
# Disable CPU frequency scaling
sudo cpupower frequency-set -g performance

# Run benchmarks
cargo bench --bench "ai_*"

# Re-enable scaling
sudo cpupower frequency-set -g powersave
```

## Implementation Details

All benchmarks use:
- **Dataset Generation**: Deterministic, compressible synthetic data
- **Compression**: LZ4 (fast decompression, ~60% compression ratio)
- **Block Size**: Aligned to sample sizes
- **Seeding**: Fixed seeds for reproducibility

See `common.rs` for shared helper functions.

## Contributing

When adding new benchmarks:

1. Document the scenario being tested
2. Use realistic parameters (match PyTorch/TensorFlow defaults)
3. Include throughput measurements
4. Update this document

## References

- Full documentation: `README.md`
- Benchmark source: `crates/cli/benches/ai/`
- Common helpers: `crates/cli/benches/ai/common.rs`
