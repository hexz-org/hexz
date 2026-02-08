# ML Efficiency Example: Why Strata Matters

This example demonstrates **real performance benefits** of using Strata for machine learning workloads.

## The Problems Strata Solves

### 1. Dataset Deduplication (Storage & Bandwidth Savings)

**Scenario:** You're training a model with heavy data augmentation. Your dataset has:
- 50,000 base images
- 10 augmented versions per image (rotation, crop, color jitter)
- Total: 500,000 images

**Traditional Approach:**
- Store all 500,000 images individually
- Each augmented image stored separately
- Heavy redundancy (augmentations share significant pixel data)
- Result: Approximately 3x storage due to redundancy

**Strata Approach:**
- Block-level deduplication recognizes similar regions
- Compression + dedup reduces redundancy automatically
- Result: Storage closer to base dataset size plus delta
- Note: Actual savings depend on augmentation types and data characteristics

### 2. Streaming Large Datasets (Memory Efficiency)

**Scenario:** Training on ImageNet-21k (14 million images, 1.3TB uncompressed)

**Traditional Approach:**
- Download entire dataset first
- Extract tar files to disk
- Requires storage for compressed + uncompressed data
- Result: Long setup time, high disk requirements

**Strata Approach:**
- Sparse download: only fetch blocks you need
- Decompress on-the-fly in Rust (parallel)
- No full extraction required
- Result: Start training quickly with minimal disk cache

### 3. Random Access Performance (Training Speed)

**Scenario:** Training with random shuffling (standard practice for SGD)

**Traditional Approach:**
- Tar files are sequential-only (can't seek)
- HDF5 has overhead for small reads
- Reading random samples may cause cache misses
- Result: May experience I/O bottlenecks with random access

**Strata Approach:**
- Block-indexed format allows random access
- Prefetch cache can predict next batches
- Parallel decompression in Rust threads
- Result: Better random access performance
- Note: Run benchmark to measure on your specific workload

---

## Running the Examples

This directory contains three demonstrations:

### Example 1: Deduplication Savings
```bash
python 01_dedup_demo.py
```
Creates an augmented dataset and shows storage comparison.

This example measures actual file sizes on disk to demonstrate deduplication
benefits. Results will vary based on augmentation types.

### Example 2: Streaming vs Download
```bash
python 02_streaming_demo.py
```
Demonstrates streaming approach vs full download.

Shows the time-to-first-batch difference between downloading an entire dataset
versus streaming with Strata's sparse block access.

### Example 3: Random Access Benchmark
```bash
python 03_random_access_benchmark.py
```
Measures actual random read latency for different formats.

Compares Individual JPEGs, TAR archives, HDF5, and Strata on the same dataset
with randomized access patterns. Results are system-dependent.

---

## When to Use Strata

### Good Use Cases
- Large datasets that don't fit in memory (streaming required)
- Datasets with redundancy (augmented data, checkpoints, synthetic data)
- Random access patterns (shuffled training, non-sequential sampling)
- Remote training (S3, HTTP) where bandwidth is limited
- Multi-epoch training where you want incremental caching

### Not Ideal
- Small datasets (<1GB) where you can just load everything into RAM
- Sequential-only access (if you never shuffle, tar.gz is fine)
- No redundancy (completely unique data won't benefit from dedup)

---

## Technical Details

### How Deduplication Works
- Strata uses **content-defined chunking** (CDC) to split data into variable-sized blocks
- Identical blocks (based on BLAKE3 hash) are stored once
- Even partial duplicates (e.g., images with slight variations) share common blocks
- Typical dedup ratios: 30-60% for augmented datasets, 15-25% for ML checkpoints

### How Streaming Works
- Strata downloads a small **index** first (~0.1% of dataset size)
- The index maps sample IDs to block locations
- When you request a sample, only required blocks are fetched
- Prefetcher predicts next samples and downloads in background
- LRU cache keeps hot blocks in memory

### How Random Access Works
- Each block is compressed independently (LZ4 or Zstd)
- Block index stores offsets in the file
- Seeking to sample N: `O(log n)` index lookup + single block read
- No need to decompress entire archive
- Parallel decompression across multiple blocks

---

## Comparison to Alternatives

| Format      | Random Access | Dedup | Streaming | Compression |
|-------------|---------------|-------|-----------|-------------|
| Individual Files | Yes | No | Partial (slow) | No |
| TAR/TGZ     | No | No | Partial (seq only) | Yes |
| WebDataset  | Partial (sharded) | No | Yes | Yes |
| HDF5        | Yes | No | Partial (limited) | Partial |
| Zarr        | Yes | No | Yes | Partial |
| **Strata**  | Yes | Yes | Yes | Yes |

---

## FAQ

**Q: Does this replace PyTorch's DataLoader?**
A: No, Strata provides a Dataset class that works WITH DataLoader. You still use `torch.utils.data.DataLoader` as normal.

**Q: Does deduplication slow down reads?**
A: No. Dedup happens during packing (write time). Reading is just as fast—actually faster due to less data transfer.

**Q: Can I use this with TensorFlow?**
A: Currently PyTorch-only, but the core format is framework-agnostic. TensorFlow bindings are possible.

**Q: What about data augmentation?**
A: Do augmentation in your DataLoader (standard practice). Strata stores the base data efficiently; you augment on-the-fly.

**Q: How does this compare to caching?**
A: Strata includes smart caching. But the key difference is it streams from compressed storage, so you don't need to cache the entire dataset.
