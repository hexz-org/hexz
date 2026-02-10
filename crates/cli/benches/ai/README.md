# Strata AI/ML Benchmarks

Comprehensive benchmark suite for validating Strata's performance on AI and machine learning workloads. These benchmarks simulate realistic ML training scenarios including PyTorch DataLoader and TensorFlow tf.data patterns.

**From the repo root**, the central entry point is the **Makefile**: **`make bench`** runs the main Criterion benchmarks. The sections below document the **AI-specific** benchmark harness and give the exact **`cargo bench`** invocations for fine-grained control (e.g. a single bench, filters, baselines). Run all commands from the **repository root**.

## Overview

Strata is designed as a high-performance data loading backend for ML training pipelines. These benchmarks validate that design by measuring:

- **Data loading throughput** under various access patterns
- **Multi-worker scalability** for parallel data loading
- **Cache effectiveness** across training epochs
- **Prefetching strategies** for hiding I/O latency
- **Tensor operation overhead** for zero-copy transfers
- **End-to-end training workflows** with realistic parameters

## Benchmark Categories

### 1. Data Loader Performance (`dataloader.rs`)

Simulates PyTorch/TensorFlow DataLoader behavior across different workload patterns:

- **Sequential Iteration**: Measures throughput when reading samples in order (epoch without shuffling)
- **Random Access**: Tests shuffled epoch performance with random sample ordering
- **Batch Loading**: Evaluates batching efficiency with varying batch sizes (1-128)
- **Sample Size Scaling**: Tests different tensor sizes from 1KB (text) to 1MB (high-res images)
- **Cache Warmup**: Quantifies cold cache vs. warm cache performance across epochs

**Run with:**
```bash
cargo bench --bench ai_dataloader
```

**Key metrics:**
- Throughput (MB/s) for different access patterns
- Impact of sample size on I/O efficiency
- Cache hit rate improvement on repeated epochs

### 2. Index Shuffling (`shuffle.rs`)

Measures Fisher-Yates shuffling performance at scale:

- **Scaling**: Tests shuffle performance from 1K to 10M samples
- **Component Breakdown**: Separates allocation cost from permutation cost
- **PRNG Comparison**: Compares xorshift64 vs. alternatives (quality/performance tradeoff)
- **Access Pattern Impact**: Measures cache locality cost of shuffled vs. sequential access

**Run with:**
```bash
cargo bench --bench ai_shuffle
```

**Key metrics:**
- Shuffle time scaling (should be O(n))
- Memory allocation overhead
- Cache miss penalty for random access patterns

### 3. Prefetching Strategies (`prefetch.rs`)

Evaluates prefetching effectiveness for reducing I/O latency:

- **Window Size Tuning**: Tests prefetch windows from 0 (no prefetch) to 32 blocks ahead
- **Strided Access**: Measures prefetch utility with non-sequential patterns
- **Adaptive Prefetching**: Compares adaptive vs. fixed-window strategies
- **Block Size Interaction**: Tests how block size affects prefetch efficiency
- **Hit Rate Simulation**: Estimates cache hit rates with different prefetch parameters

**Run with:**
```bash
cargo bench --bench ai_prefetch
```

**Key metrics:**
- Optimal prefetch window size for sequential workloads
- Prefetch effectiveness for strided/sparse access
- Memory overhead vs. latency reduction tradeoff

### 4. Multi-Worker Loading (`multiworker.rs`)

Simulates PyTorch num_workers parameter (parallel data loading):

- **Worker Scaling**: Tests throughput with 1-16 parallel workers
- **Contention Analysis**: Measures overhead when workers access overlapping data
- **Distribution Strategies**: Tests round-robin and partitioned workload distribution
- **Load Balancing**: Evaluates performance with imbalanced work distribution
- **Lifecycle Overhead**: Measures worker spawn/join cost per epoch

**Run with:**
```bash
cargo bench --bench ai_multiworker
```

**Key metrics:**
- Throughput scaling with worker count
- Optimal number of workers for different dataset sizes
- Contention overhead with shared data access

### 5. Tensor Operations (`tensor_ops.rs`)

Measures overhead of tensor-specific operations:

- **Size Scaling**: Tests common tensor sizes (MNIST, CIFAR, ImageNet, etc.)
- **Zero-Copy vs. Copy**: Quantifies benefit of zero-copy buffer protocol
- **Batch Tensor Loading**: Measures batching efficiency for tensor reads
- **Preprocessing Overhead**: Tests common transforms (reshape, normalize, transpose)
- **Alignment**: Evaluates SIMD/GPU alignment requirements
- **Concatenation**: Measures cost of combining multiple tensors

**Run with:**
```bash
cargo bench --bench ai_tensor_ops
```

**Key metrics:**
- Zero-copy savings vs. memcpy overhead
- Optimal batch size for tensor loading
- Preprocessing cost relative to I/O time

### 6. ML Training Workloads (`ml_workloads.rs`)

End-to-end realistic training scenarios:

- **Multi-Epoch Training**: Simulates full training loop with shuffling between epochs
- **Train/Val Split**: Tests combined training and validation data access
- **Batch Size Scaling**: Measures impact of batch size on overall throughput
- **Checkpoint/Resume**: Tests seeking to arbitrary dataset positions
- **Data Augmentation**: Measures I/O vs. compute tradeoff with transforms
- **Drop Last Behavior**: Tests impact of incomplete final batches
- **Subset Sampling**: Evaluates sparse random access for validation subsets

**Run with:**
```bash
cargo bench --bench ai_ml_workloads
```

**Key metrics:**
- End-to-end training epoch time
- I/O time as percentage of total training time
- Cache effectiveness across multiple epochs

## Running All AI Benchmarks

Run the complete AI benchmark suite:

```bash
cargo bench --bench "ai_*"
```

Run a specific benchmark category:
```bash
cargo bench --bench ai_dataloader -- "Sequential"
cargo bench --bench ai_shuffle -- "Scaling"
cargo bench --bench ai_multiworker -- "workers/8"
```

## Interpreting Results

### Throughput Metrics

All benchmarks report throughput in **MB/s** or **samples/s**. Higher is better.

**Good performance indicators:**
- Sequential access: >500 MB/s on SSD
- Random access (shuffled): >200 MB/s on SSD
- Multi-worker scaling: Near-linear up to 8 workers
- Cache hit rate: >90% on second epoch

### Comparison Baselines

These benchmarks can be compared against:
- **PyTorch DataLoader**: Run with `persistent_workers=True, num_workers=8`
- **TensorFlow tf.data**: Run with `AUTOTUNE` and prefetch
- **Raw filesystem**: Using `fio` or `dd` for sequential/random I/O
- **Memory-mapped files**: Using `mmap` with madv_sequential

### Performance Goals

Target performance tiers for ML workloads:

| Metric | Target | Excellent |
|--------|--------|-----------|
| Sequential throughput | >200 MB/s | >500 MB/s |
| Random access (shuffled) | >100 MB/s | >300 MB/s |
| Shuffle 1M samples | <50ms | <20ms |
| Worker scaling (8 workers) | >4x speedup | >7x speedup |
| Cache hit rate (epoch 2+) | >80% | >95% |
| Multi-epoch overhead | <5% vs. single | <2% vs. single |

## Implementation Details

### Test Data Generation

All benchmarks use deterministic, moderately compressible synthetic data:
- Seeded PRNG for reproducibility
- ~60% compression ratio (typical for real images)
- LZ4 compression (fast decompression)
- Block sizes aligned to typical tensor dimensions

### Benchmark Configuration

```rust
// Typical benchmark setup
let num_samples = 10000;        // Dataset size
let sample_size = 4096;         // 4KB per sample (small image)
let batch_size = 32;            // Standard training batch
let num_epochs = 5;             // Multi-epoch training
let num_workers = 8;            // Parallel data loaders
let prefetch_window = 4;        // Blocks to prefetch ahead
```

### Cache Behavior

Benchmarks test multiple cache scenarios:
- **Cold cache**: First access to data
- **Warm cache**: Repeated access within epoch
- **Inter-epoch cache**: Data retained across epochs
- **LRU eviction**: Cache pressure with large datasets

## Contributing

When adding new AI benchmarks:

1. **Document the scenario**: What real-world ML workflow does this simulate?
2. **Use realistic parameters**: Match PyTorch/TensorFlow defaults
3. **Measure variance**: Use Criterion's statistical rigor
4. **Add to this README**: Update the appropriate section

## Common Use Cases

### Tuning for Your Workload

```bash
# Find optimal batch size for your tensor size
cargo bench --bench ai_ml_workloads -- "BatchSize"

# Find optimal prefetch window
cargo bench --bench ai_prefetch -- "WindowSize"

# Find optimal number of data loader workers
cargo bench --bench ai_multiworker -- "Scaling"
```

### Regression Testing

```bash
# Establish baseline
cargo bench --bench "ai_*" -- --save-baseline ai_baseline

# After changes, compare
cargo bench --bench "ai_*" -- --baseline ai_baseline
```

### Profiling

```bash
# Profile with perf
cargo bench --bench ai_dataloader --profile-time 10

# Profile with flamegraph
cargo flamegraph --bench ai_dataloader
```

## Architecture Notes

These benchmarks are designed to stress-test Strata's core abstractions:

- **StorageBackend**: Tests local, HTTP, and S3 backends under ML access patterns
- **Cache Layer**: Validates LRU cache effectiveness for multi-epoch training
- **Prefetching**: Tests adaptive and fixed prefetching strategies
- **Compression**: Measures decompression overhead vs. I/O reduction
- **Concurrency**: Validates thread-safe access with minimal contention

The goal is to ensure Strata can **match or exceed** native PyTorch/TensorFlow
data loading performance while providing additional benefits:
- **Compression**: Reduce storage and network costs
- **Deduplication**: Share data across training runs
- **Remote Access**: Stream from HTTP/S3 without local copies
- **Snapshots**: Version control for datasets
- **Encryption**: Secure sensitive training data

## References

- [PyTorch DataLoader Documentation](https://pytorch.org/docs/stable/data.html)
- [TensorFlow tf.data Best Practices](https://www.tensorflow.org/guide/data_performance)
- [MLPerf Training Rules](https://mlcommons.org/en/training-normal-11/)
- [NVIDIA DALI](https://github.com/NVIDIA/DALI) - GPU-accelerated data loading
- [WebDataset](https://github.com/webdataset/webdataset) - Tar-based dataset format
