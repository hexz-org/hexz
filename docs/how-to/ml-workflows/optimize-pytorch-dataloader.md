# Optimize PyTorch DataLoader Performance

**Goal**: Maximize training throughput when loading data from Strata snapshots.

## Prerequisites

- Strata Python package installed
- PyTorch installed
- Basic understanding of `torch.utils.data.DataLoader`

## Problem

Default DataLoader settings often underutilize Strata's capabilities, leading to GPU starvation and slow training.

## Solution

Apply these optimizations in order, measuring impact at each step.

## Step 1: Increase Worker Count

DataLoader workers decompress blocks in parallel.

**Default (slow)**:
```python
loader = DataLoader(dataset, batch_size=32, num_workers=0)  # Single-threaded
```

**Optimized**:
```python
loader = DataLoader(
    dataset,
    batch_size=32,
    num_workers=4,  # Start with 4, increase to 8
    pin_memory=True  # If using GPU
)
```

**Rule of thumb**: Set `num_workers` to number of CPU cores available, up to 8-12.

**Benchmark**:
```python
import time

# Test with different worker counts
for num_workers in [0, 2, 4, 8]:
    loader = DataLoader(dataset, batch_size=32, num_workers=num_workers)
    start = time.time()
    for i, batch in enumerate(loader):
        if i == 100:
            break
    elapsed = time.time() - start
    print(f"num_workers={num_workers}: {100/elapsed:.1f} batches/sec")
```

## Step 2: Increase Cache Size

Strata caches decompressed blocks. Larger cache improves hit rate.

**Default**:
```python
dataset = strata.open("s3://bucket/dataset.st")  # 256MB cache
```

**Optimized**:
```python
dataset = strata.open(
    "s3://bucket/dataset.st",
    cache_size=2 * 1024**3  # 2GB cache
)
```

**Sizing guideline**: Set cache to 1-2x your batch memory footprint. For 32-batch of 224x224 images:
- Uncompressed: 32 * 3 * 224 * 224 = 48MB per batch
- Cache recommendation: 512MB - 1GB

## Step 3: Enable Disk Cache for Multi-Epoch Training

Persist cache across runs to speed up subsequent epochs.

```python
dataset = strata.open(
    "s3://bucket/dataset.st",
    cache_size=2 * 1024**3,
    cache_dir="/tmp/strata-cache"  # Persists to disk
)
```

**Impact**: First epoch downloads from S3, subsequent epochs read from local disk.

## Step 4: Prefetch Factor

PyTorch can prefetch batches ahead of time.

```python
loader = DataLoader(
    dataset,
    batch_size=32,
    num_workers=4,
    prefetch_factor=2,  # Prefetch 2 batches per worker (default)
    pin_memory=True
)
```

Increase to 4 if you have spare memory:
```python
prefetch_factor=4  # 4 batches * 4 workers = 16 batches prefetched
```

## Step 5: Persistent Workers

Avoid worker restart overhead.

```python
loader = DataLoader(
    dataset,
    batch_size=32,
    num_workers=4,
    persistent_workers=True  # Keep workers alive between epochs
)
```

**Impact**: Eliminates worker spawn overhead at epoch boundaries (saves 1-2 seconds per epoch).

## Step 6: Batch Size Tuning

Larger batches amortize decompression overhead.

```python
# Small batches (more overhead)
loader = DataLoader(dataset, batch_size=16, num_workers=4)

# Larger batches (less overhead per sample)
loader = DataLoader(dataset, batch_size=64, num_workers=4)
```

**Trade-off**: Larger batches use more GPU memory and may hurt convergence.

## Step 7: Pin Memory (GPU Training)

Transfer batches to GPU faster.

```python
loader = DataLoader(
    dataset,
    batch_size=32,
    num_workers=4,
    pin_memory=True  # Allocate in pinned memory
)

# In training loop
for batch in loader:
    batch = batch.to(device, non_blocking=True)  # Async transfer
```

## Complete Optimized Configuration

```python
import torch
from torch.utils.data import DataLoader
import strata

# Open dataset with large cache
dataset = strata.open(
    "s3://bucket/imagenet.st",
    s3_region="us-west-2",
    cache_size=2 * 1024**3,      # 2GB cache
    cache_dir="/tmp/strata-cache"
)

# Create optimized DataLoader
loader = DataLoader(
    dataset,
    batch_size=64,                # Larger batches
    shuffle=True,
    num_workers=8,                # Parallel workers
    prefetch_factor=4,            # Prefetch 4 batches per worker
    persistent_workers=True,      # Keep workers alive
    pin_memory=True               # For GPU training
)

# Training loop
device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
for epoch in range(10):
    for batch in loader:
        batch = batch.to(device, non_blocking=True)
        # Your training code
        pass
```

## Measuring Impact

```python
import time

def benchmark_dataloader(loader, num_batches=100):
    start = time.time()
    for i, batch in enumerate(loader):
        if i >= num_batches:
            break
    elapsed = time.time() - start
    return num_batches / elapsed

# Baseline
loader_baseline = DataLoader(dataset, batch_size=32, num_workers=0)
baseline_speed = benchmark_dataloader(loader_baseline)

# Optimized
loader_optimized = DataLoader(
    dataset, batch_size=64, num_workers=8,
    prefetch_factor=4, persistent_workers=True, pin_memory=True
)
optimized_speed = benchmark_dataloader(loader_optimized)

print(f"Baseline: {baseline_speed:.1f} batches/sec")
print(f"Optimized: {optimized_speed:.1f} batches/sec")
print(f"Speedup: {optimized_speed/baseline_speed:.2f}x")
```

## Expected Performance

Typical improvements from optimization:

| Configuration | Batches/sec | Speedup |
|--------------|-------------|---------|
| Baseline (1 worker, small cache) | 10-15 | 1.0x |
| + 4 workers | 35-45 | 3-4x |
| + 8 workers | 55-70 | 5-7x |
| + Large cache | 75-90 | 7-9x |
| + Persistent workers | 80-95 | 8-9x |

Actual performance depends on CPU, network, and data complexity.

## Troubleshooting

**Workers not improving performance**:
- Check CPU usage: `htop` should show multiple cores active
- Ensure GIL is released (Strata does this automatically)
- Try different worker counts

**High memory usage**:
- Reduce `cache_size`
- Reduce `num_workers`
- Reduce `prefetch_factor`

**Slow first epoch, fast subsequent epochs**:
- Normal behavior with S3 streaming
- Enable `cache_dir` to persist cache

## See Also

- [Setup S3 Streaming](setup-s3-streaming.md)
- [Performance Tuning](../performance-tuning.md)
- [Reference: Python API](../../reference/python-api.md)
