# Performance Tuning Guide

**Goal**: Maximize Hexz performance for your specific workload.

## Prerequisites

- Hexz installed and working
- Basic performance measurement tools (time, htop, iostat)

## Performance Bottlenecks

Identify your bottleneck before optimizing:

| Symptom | Likely Bottleneck | Solution Section |
|---------|-------------------|------------------|
| Low CPU usage | Not enough parallelism | Worker Count |
| High CPU usage (100%) | Too many workers | Reduce workers |
| High memory usage | Cache too large | Cache Sizing |
| Slow first epoch, fast after | Network/S3 latency | Prefetching, Disk Cache |
| Slow all epochs | Decompression overhead | Compression Algorithm |

## Worker Count Tuning

More workers improve throughput until CPU or memory saturates.

**Find optimal worker count**:
```python
import time
import torch
from torch.utils.data import DataLoader

def benchmark(num_workers):
    loader = DataLoader(dataset, batch_size=32, num_workers=num_workers)
    start = time.time()
    for i, batch in enumerate(loader):
        if i == 100:
            break
    return 100 / (time.time() - start)

for workers in [0, 2, 4, 8, 12, 16]:
    speed = benchmark(workers)
    print(f"workers={workers:2d}: {speed:6.1f} batches/sec")
```

**Typical optimal**: 4-8 workers for local files, 8-12 for S3 streaming

## Cache Sizing

Larger cache reduces repeated decompression but uses more memory.

**Calculate optimal cache size**:
```python
# Estimate working set size
dataset_size = reader.size()  # Total size
samples_per_epoch = len(dataset)
sample_size = dataset_size / samples_per_epoch

# Working set (samples touched per epoch)
# With shuffling, typically 30-50% on first pass
working_set_size = dataset_size * 0.4

# Recommended cache size: 50-100% of working set
cache_size = int(working_set_size * 0.5)

dataset = hexz.open(path, cache_size=cache_size)
```

**Memory-constrained systems**:
```python
import psutil

# Use 25% of available RAM for cache
available_ram = psutil.virtual_memory().available
cache_size = int(available_ram * 0.25)

dataset = hexz.open(path, cache_size=cache_size)
```

## Compression Algorithm Selection

Choose based on access pattern:

| Algorithm | Decompress Speed | Ratio | Use Case |
|-----------|------------------|-------|----------|
| LZ4 | Very fast | 2-3x | Hot data, frequent access, local NVMe |
| Zstandard (level 3) | Fast | 3-4x | Warm data, S3 streaming |
| Zstandard (level 9) | Moderate | 4-5x | Cold storage, archival |

**Repack with different compression**:
```bash
# Current: Zstd level 9 (high compression, slower)
# Change to: LZ4 (fast decompression)

hexz data pack \
  --disk original.hxz \
  --output optimized.hxz \
  --compression lz4
```

## Block Size Tuning

Larger blocks compress better but slower random access.

| Block Size | Random Access | Compression | Use Case |
|------------|---------------|-------------|----------|
| 4 KB | Fastest | Worst | VM boot, tiny random reads |
| 64 KB | Fast | Good | Default, balanced |
| 256 KB | Moderate | Better | Sequential access patterns |
| 1 MB | Slow | Best | Cold storage, streaming-only |

**Repack with optimal block size**:
```bash
# For random access workload
hexz data pack \
  --disk data/ \
  --output fast-access.hxz \
  --block-size 16384  # 16KB

# For sequential streaming
hexz data pack \
  --disk data/ \
  --output high-compression.hxz \
  --block-size 262144  # 256KB
```

## S3 Optimization

### Region Locality

Ensure region matches:
```python
# Check bucket region
import boto3
s3 = boto3.client('s3')
location = s3.get_bucket_location(Bucket='my-bucket')
print(f"Bucket region: {location['LocationConstraint']}")

# Use matching region
dataset = hexz.open(
    "s3://my-bucket/dataset.hxz",
    s3_region=location['LocationConstraint']
)
```

### Connection Timeout Tuning

```python
dataset = hexz.open(
    "s3://bucket/dataset.hxz",
    connect_timeout=15,  # Increase if network is slow
    read_timeout=60,     # Increase for large block reads
    retry_attempts=5     # Retry on transient failures
)
```

### S3 Transfer Acceleration

Enable on bucket, then use accelerated endpoint:
```bash
# Enable on bucket
aws s3api put-bucket-accelerate-configuration \
    --bucket my-bucket \
    --accelerate-configuration Status=Enabled
```

## Disk Cache for Multi-Epoch Training

Persist cache to disk for faster subsequent epochs:

```python
dataset = hexz.open(
    "s3://bucket/dataset.hxz",
    cache_size=2 * 1024**3,  # 2GB in-memory cache
    cache_dir="/nvme/hexz-cache"  # Fast local disk
)
```

**Cache directory performance**:
- NVMe SSD: Best (5GB/s)
- SATA SSD: Good (500MB/s)
- HDD: Acceptable (150MB/s)
- Network: Not recommended

## DataLoader Optimizations

See [Optimize PyTorch DataLoader](ml-workflows/optimize-pytorch-dataloader.md) for complete guide.

Quick wins:
```python
loader = DataLoader(
    dataset,
    batch_size=64,  # Larger batches
    num_workers=8,  # More workers
    prefetch_factor=4,  # Prefetch more
    persistent_workers=True,  # Keep workers alive
    pin_memory=True  # For GPU training
)
```

## Monitoring Performance

### System Metrics

```bash
# CPU usage
htop

# Disk I/O
iostat -x 1

# Network
iftop

# Memory
vmstat 1
```

### Hexz Metrics

```python
import hexz
import time

reader = hexz.open("dataset.hxz")

# Measure read latency
start = time.time()
for i in range(1000):
    reader.read(4096, offset=i * 1024 * 1024)
elapsed = time.time() - start
print(f"Average latency: {elapsed/1000*1000:.2f}ms")

# Measure throughput
start = time.time()
total_bytes = 0
for i in range(1000):
    data = reader.read(64 * 1024, offset=i * 64 * 1024)
    total_bytes += len(data)
elapsed = time.time() - start
throughput = total_bytes / elapsed / 1024 / 1024
print(f"Throughput: {throughput:.1f} MB/s")
```

## Hardware Recommendations

### For ML Training

**CPU**: 8+ cores for parallel decompression
**RAM**: 32GB+ (16GB for dataset cache, 16GB for PyTorch)
**Storage**: NVMe SSD for local cache
**Network**: 10Gbps for S3 streaming

### For VM Boot

**CPU**: 4+ cores
**RAM**: 8GB+ for VMs, 2GB for Hexz cache
**Storage**: SSD strongly recommended
**KVM**: Hardware virtualization support

## Benchmark Results

Validated performance on Intel i7-14700K, 64GB RAM, NVMe SSD.

**See [`docs/project-docs/BENCHMARKS.md`](../project-docs/BENCHMARKS.md)** for complete benchmark results including:

- Compression/decompression throughput (LZ4 and Zstd)
- Sequential and random access performance
- Multi-worker scaling characteristics
- AI/ML DataLoader simulation results
- Cache effectiveness measurements

### Quick Reference

| Metric | Value | Benchmark |
|--------|-------|-----------|
| LZ4 Compress | 22 GB/s | `cargo bench --bench compression` |
| LZ4 Decompress | 31 GB/s | `cargo bench --bench compression` |
| Sequential Read | 8-9 GB/s | `cargo bench --bench read_throughput` |
| Random Access (cold) | 6.6 µs | `cargo bench --bench sparse_access` |
| Random Access (warm) | 174 ns | `cargo bench --bench sparse_access` |
| Multi-worker (8 workers) | 2.9× speedup | `cargo bench --bench ai_multiworker` |

### Not Yet Validated

**S3 Streaming**, **Deduplication Efficiency**, and **Real-world Compression Ratios** need benchmarks. See BENCHMARKS.md "What's Not Yet Validated" section.

## Troubleshooting Slow Performance

**Step 1: Identify bottleneck**
```bash
# Run training with monitoring
htop  # Watch CPU usage
iotop  # Watch disk I/O
iftop  # Watch network
```

**Step 2: Check configuration**
```python
# Print current configuration
import hexz
reader = hexz.open("dataset.hxz")
print(f"Cache size: {reader.cache_size / 1024**3:.1f} GB")
print(f"Cache hit rate: {reader.cache_hits / (reader.cache_hits + reader.cache_misses):.1%}")
```

**Step 3: Apply optimizations**

If CPU < 50%: Increase `num_workers`
If CPU = 100%: Decrease `num_workers`
If memory high: Decrease `cache_size`
If network slow: Use disk cache, or switch compression to LZ4

## See Also

- [How-To: Optimize PyTorch DataLoader](ml-workflows/optimize-pytorch-dataloader.md)
- [How-To: Setup S3 Streaming](ml-workflows/setup-s3-streaming.md)
- [Reference: Configuration](../reference/configuration.md)
- [Explanation: Architecture](../explanation/architecture.md)
