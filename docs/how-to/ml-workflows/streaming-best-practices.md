# Streaming Best Practices

**Goal**: Optimize Strata for production ML data streaming workflows.

## Prerequisites

- Strata Python package installed
- Experience with PyTorch DataLoader
- Production ML training environment

## Best Practices

### 1. Region Locality

**Always match S3 bucket region with training instance region**.

```python
# Check bucket region first
import boto3
s3 = boto3.client('s3')
location = s3.get_bucket_location(Bucket='my-bucket')
print(location['LocationConstraint'])

# Use matching region
dataset = strata.open(
    "s3://my-bucket/dataset.st",
    s3_region=location['LocationConstraint']
)
```

**Impact**: Cross-region adds 50-150ms latency per request.

### 2. Cache Sizing

**Size cache to working set, not full dataset**.

```python
# Calculate working set (30-50% of dataset typically accessed per epoch)
dataset_size = 1 * 1024**4  # 1TB
working_set = dataset_size * 0.4  # 400GB typical

# Set cache to 10-20% of working set (fits in RAM)
cache_size = int(working_set * 0.15)  # 60GB

dataset = strata.open(
    "s3://bucket/dataset.st",
    cache_size=cache_size,
    cache_dir="/nvme/cache"  # Overflow to fast disk
)
```

### 3. Worker Parallelism

**Match workers to CPU cores and batch size**.

```python
import os

# CPU-bound: workers = cores
num_workers = os.cpu_count()

# Memory-bound: reduce workers
available_ram = psutil.virtual_memory().available
batch_memory = batch_size * sample_size
max_workers = available_ram // (batch_memory * prefetch_factor)

num_workers = min(os.cpu_count(), max_workers)

loader = DataLoader(dataset, num_workers=num_workers)
```

### 4. Persistent Workers

**Keep workers alive between epochs**.

```python
loader = DataLoader(
    dataset,
    batch_size=64,
    num_workers=8,
    persistent_workers=True  # Avoid respawn overhead
)
```

**Savings**: 1-2 seconds per epoch.

### 5. Prefetch Tuning

**Balance memory vs. pipeline depth**.

```python
loader = DataLoader(
    dataset,
    batch_size=64,
    num_workers=8,
    prefetch_factor=2  # 2 batches per worker = 16 batches total
)
```

**Memory usage**: `batch_size * num_workers * prefetch_factor * sample_size`

### 6. Pin Memory for GPU Training

**Enable pinned memory for faster CPU-GPU transfer**.

```python
loader = DataLoader(
    dataset,
    batch_size=64,
    pin_memory=True  # Requires CUDA
)

# In training loop
for batch in loader:
    batch = batch.to(device, non_blocking=True)  # Async transfer
```

### 7. Monitor Cache Hit Rate

**Track cache efficiency**.

```python
import time

start_time = time.time()
total_reads = 0
cache_hits = 0

for epoch in range(num_epochs):
    epoch_start = time.time()

    for batch in loader:
        total_reads += 1
        # Training...

    epoch_time = time.time() - epoch_start

    # Log metrics
    hit_rate = cache_hits / total_reads if total_reads > 0 else 0
    print(f"Epoch {epoch}: {epoch_time:.1f}s, cache hit rate: {hit_rate:.1%}")
```

Target: >90% hit rate after first epoch.

### 8. Disk Cache for Multi-Epoch Training

**Persist cache across training runs**.

```python
dataset = strata.open(
    "s3://bucket/dataset.st",
    cache_size=2 * 1024**3,  # 2GB RAM
    cache_dir="/nvme/strata-cache"  # Persist to NVMe
)
```

**Benefit**: First epoch downloads from S3, subsequent epochs use local cache.

### 9. Compression Selection

**Use Zstd for S3, LZ4 for local**.

```bash
# For S3 streaming (save bandwidth)
strata data pack \
  --disk data/ \
  --output dataset.st \
  --compression zstd \
  --compression-level 9

# For local NVMe (fast decompression)
strata data pack \
  --disk data/ \
  --output dataset.st \
  --compression lz4
```

### 10. Retry Configuration

**Handle transient S3 failures**.

```python
dataset = strata.open(
    "s3://bucket/dataset.st",
    retry_attempts=5,  # Retry on failure
    connect_timeout=15,
    read_timeout=60
)
```

## Production Checklist

- [ ] Dataset packed with deduplication (`--cdc`)
- [ ] Compression algorithm selected (Zstd for S3)
- [ ] S3 bucket region matches training region
- [ ] Cache size tuned to working set
- [ ] Disk cache enabled for multi-epoch
- [ ] Worker count optimized (4-8 typically)
- [ ] Persistent workers enabled
- [ ] Prefetch factor configured
- [ ] Pin memory enabled (GPU training)
- [ ] Monitoring and logging configured
- [ ] Retry logic configured
- [ ] Tested with representative workload

## Monitoring

Log these metrics:

- Epoch time
- Samples/second
- Cache hit rate
- Network bandwidth usage
- CPU utilization
- Memory usage

Example:
```python
import time
import psutil

for epoch in range(num_epochs):
    epoch_start = time.time()
    samples_processed = 0

    for batch in loader:
        samples_processed += len(batch)
        # Training...

    epoch_time = time.time() - epoch_start
    samples_per_sec = samples_processed / epoch_time
    cpu_percent = psutil.cpu_percent()
    mem_percent = psutil.virtual_memory().percent

    print(f"Epoch {epoch}:")
    print(f"  Time: {epoch_time:.1f}s")
    print(f"  Throughput: {samples_per_sec:.0f} samples/s")
    print(f"  CPU: {cpu_percent:.0f}%")
    print(f"  Memory: {mem_percent:.0f}%")
```

## Common Pitfalls

**Too many workers**: Causes context switching overhead, actually slows down.
**Solution**: Start with 4, increase gradually while monitoring.

**Cache too small**: Repeated S3 fetches every epoch.
**Solution**: Size cache to 10-20% of working set.

**Wrong S3 region**: Cross-region latency.
**Solution**: Always match bucket region.

**No disk cache**: Re-download from S3 every run.
**Solution**: Enable `cache_dir` on fast local disk.

**Forgotten pin_memory**: Slow CPU-GPU transfers.
**Solution**: Always enable for GPU training.

## See Also

- [How-To: Setup S3 Streaming](setup-s3-streaming.md)
- [How-To: Optimize PyTorch DataLoader](optimize-pytorch-dataloader.md)
- [How-To: Performance Tuning](../performance-tuning.md)
- [Reference: Configuration](../../reference/configuration.md)
