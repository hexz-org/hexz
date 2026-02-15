# Hexz Frequently Asked Questions (FAQ)

Common questions about Hexz, its design decisions, and how it compares to alternatives.

---

## General Questions

### What is Hexz?

Hexz is a **seekable compressed filesystem** that allows random access to compressed data with microsecond latency. Think of it as "tar.gz with instant random access" or "a database for raw bytes."

**Primary use cases:**
1. **ML/AI Training** — Stream TB-scale datasets from S3 directly to GPUs without local copies
2. **VM Management** — Boot operating systems from compressed disk images in milliseconds

### What makes Hexz different from zip/tar.gz/7z?

Traditional archives require **sequential decompression**:
- To read byte 1GB in a tar.gz file, you must decompress from byte 0
- Random access is impossible without decompressing the entire archive

Hexz uses **block-level compression**:
- Data is split into 64KB blocks, each compressed independently
- Random access: seek to block #15,000, decompress only that 64KB
- Latency: **6.6 µs** for random block access (vs. seconds for tar.gz)

### What makes Hexz different from HDF5/Zarr?

HDF5 and Zarr are designed for **array-like data** (medical imaging, scientific datasets):
- Fixed schema and chunking strategy
- High overhead for variable-sized data (images, text, videos)
- Complex directory structure adds metadata overhead

Hexz is designed for **blob/file data** (ML datasets, disk images):
- No schema required — store arbitrary bytes
- Optimized for variable-sized samples
- Flat index structure for fast lookups

### What makes Hexz different from WebDataset?

WebDataset uses **tar shards**:
- Data split into thousands of tar files (shards)
- Shuffling limited to within shards (not true global shuffling)
- Adding 1% new data requires rebalancing all shards
- Sequential access within shards (can't seek to arbitrary sample)

Hexz uses **single indexed archive**:
- One file per dataset (simpler management)
- True random access and global shuffling
- Updates only touch changed blocks (CDC deduplication)
- Seek to any sample in O(log N) time

### Can I use Hexz for things other than ML and VMs?

Yes! Hexz works for any use case requiring **random access to large compressed data**:
- **Backup systems** with deduplication
- **Content delivery** for large media files
- **Scientific datasets** (genomics, astronomy, climate data)
- **Log archives** with instant search
- **Game asset streaming**

The core primitive is: "Store lots of data compressed, access random parts quickly."

---

## Performance Questions

### How fast is Hexz?

**Read Performance** (Intel i7-14700K, LZ4 compression):
- Sequential read: **9.0 GB/s**
- Random access (warm cache): **174 ns** per lookup
- Random access (cold cache): **6.6 µs** (includes decompression)
- Multi-worker (16 workers): **10.5 GB/s** aggregate

**Write Performance**:
- LZ4 pack (no CDC): **4.9 GB/s**
- Zstd-3 pack (no CDC): **1.6 GB/s**
- LZ4 pack (with CDC): **1.9 GB/s**

See [BENCHMARKS.md](project-docs/BENCHMARKS.md) for detailed measurements.

### Is Hexz fast enough for GPU training?

**Yes, for most workloads.** Modern GPU training requires 100-500 MB/s sustained throughput:

- **A100 GPU** (1000 images/sec × 128KB/image) = **130 MB/s required**
- **Hexz sequential read**: 9.0 GB/s = **70× faster than needed**
- **Hexz with 8 workers**: 7.5 GB/s = **57× faster than needed**

The bottleneck is typically **network bandwidth** (S3 → instance), not Hexz decompression.

### Why is CDC so slow (1.9 GB/s vs 4.9 GB/s)?

Content-Defined Chunking requires computing a **rolling hash for every byte**:

```
Fixed-size chunking (4.9 GB/s):
  [Read 64KB] → [Compress] → [Write]

CDC chunking (1.9 GB/s):
  [Read byte] → [Update rolling hash] → [Check boundary] →
  [If boundary: compress chunk] → [Write]
```

The rolling hash computation (2.7 GB/s throughput) becomes the bottleneck. This is **inherent to CDC**, not a Hexz implementation issue.

**Solution:** CDC is optional! Use fixed-size blocks for speed, CDC only when deduplication justifies the cost.

### When should I use CDC vs fixed-size blocks?

Use the **DCAM model** to decide:

```bash
# Analyze a sample of your data
hexz analyze dataset.bin --cdc --output stats.json

# DCAM predicts deduplication savings
hexz dcam optimize stats.json
```

**General rules:**

| Use Case | Recommendation | Rationale |
|----------|---------------|-----------|
| **ML training** (hot data) | Fixed-size blocks | Speed critical (4.9 GB/s vs 1.9 GB/s) |
| **Append-only datasets** | Fixed-size blocks | Dedup works fine, faster |
| **Dataset with edits/insertions** | CDC | Resilient to boundary shifts |
| **VM snapshots** | CDC | OS blocks shift when files added/removed |
| **Versioned documents** | CDC | Insertions/edits common |
| **Unique data** (random) | Fixed-size blocks | No dedup benefit either way |

**Key decision point:** Do your updates involve insertions/edits in the middle of data, or just appends at the end?
- Appends only → Fixed-size is faster and dedup works great
- Insertions/edits → CDC is worth the 2.6× slowdown for better dedup

### Does CDC slow down reading/inference?

**No!** CDC only affects **packing time** (write). Once packed:
- CDC chunks decompress at the same speed as fixed-size blocks
- Random access latency is identical (6.6 µs cold, 174 ns warm)
- Multi-worker scaling is the same

**Analogy:** CDC is like choosing to compress a zip file with `-9` (slow) vs `-1` (fast). Once compressed, extraction speed is the same.

---

## Technical Questions

### What is DCAM and why do I need it?

**DCAM** = Deduplication Change-Estimation Analytical Model

It **predicts CDC effectiveness without running full CDC**:

```python
# Without DCAM: Try different CDC parameters, pack entire dataset 5 times
for params in [4KB, 8KB, 16KB, 32KB, 64KB]:
    pack(dataset, params)  # 1 hour each = 5 hours total

# With DCAM: Analyze sample, predict optimal parameters analytically
stats = analyze_sample(dataset, sample_size=100MB)  # 10 seconds
optimal = dcam.optimize(stats)  # Instant prediction
pack(dataset, optimal)  # 1 hour (only pack once)
```

DCAM tells you:
- **Should I use CDC?** (Will savings justify the 2.6× slowdown?)
- **What chunk size?** (4KB vs 64KB — granularity vs overhead trade-off)
- **Expected savings?** (Predict "40% dedup" before packing)

**You don't NEED DCAM** — it's an optimization tool for parameter tuning.

### What compression algorithms does Hexz support?

| Algorithm | Compress | Decompress | Use Case |
|-----------|----------|------------|----------|
| **LZ4** | 23.6 GB/s | 32.1 GB/s | Hot data, ML training (default) |
| **Zstd-3** | 9.0 GB/s | 13.4 GB/s | Balanced compression/speed |
| **Zstd-9** | 1.9 GB/s | 13.4 GB/s | Maximum compression, cold storage |

Choose based on access frequency:
- **Training actively?** → LZ4 (fastest decompression)
- **Archiving?** → Zstd-9 (best ratio)
- **Balanced?** → Zstd-3

### Can Hexz deduplicate across multiple files/snapshots?

**Yes — thin snapshots** already support parent-child deduplication. A child snapshot references its parent and only stores blocks that changed:

```bash
hexz pack v1/ -o v1.hxz
hexz pack v2/ -o v2.hxz --parent v1.hxz  # Only stores blocks not in v1
```

Blocks that haven't changed are read from the parent at access time, so v2 is much smaller.

**Unrelated snapshots:** Deduplication currently works within a single pack operation. Cross-snapshot deduplication across unrelated snapshots (via a shared external index) is planned for v0.3.0.

### How does Hexz handle encryption?

**Block-level encryption** with AES-256-GCM:

```bash
hexz pack dataset/ -o encrypted.hxz --encrypt --key-file key.bin
```

**Trade-offs:**
- YES Each block encrypted independently (random access preserved)
- YES Fast: 2.1 GB/s encrypt/decrypt (AES-NI hardware acceleration)
- NO Disables deduplication (encrypted blocks differ due to unique nonces)
- NO Slightly larger (16-byte auth tag per 64KB block)

**When to encrypt:**
- Sensitive data on untrusted storage (S3, backup servers)
- Compliance requirements (HIPAA, GDPR)

**When to skip:**
- Data already public or non-sensitive
- Using storage-side encryption (AWS S3 SSE)

### Does Hexz work on Windows/macOS/Linux?

**Core library:** Yes, Rust is cross-platform (all OSes supported)

**Python bindings:** Yes (via PyO3, works on all platforms)

**VM features (FUSE mounting):** Linux only (requires libfuse)
- Workaround on macOS: Use macFUSE
- Windows: VM features not yet supported (use WSL2)

### How does Hexz integrate with PyTorch/TensorFlow?

**PyTorch:**
```python
import hexz
import torch

class HexzDataset(torch.utils.data.Dataset):
    def __init__(self, path):
        self.reader = hexz.open(path)

    def __getitem__(self, idx):
        # Hexz handles seeking, decompression, caching
        data = self.reader.read_sample(idx)
        return transform(data)

# Multi-worker DataLoader decompresses in parallel
loader = torch.utils.data.DataLoader(
    HexzDataset("s3://bucket/data.hxz"),
    batch_size=32,
    num_workers=8  # Parallelism in Rust (GIL-free)
)
```

**TensorFlow:**
```python
import hexz
import tensorflow as tf

def generator():
    reader = hexz.open("data.hxz")
    for idx in range(len(reader)):
        yield reader.read_sample(idx)

dataset = tf.data.Dataset.from_generator(
    generator,
    output_signature=tf.TensorSpec(shape=(224,224,3), dtype=tf.uint8)
)
```

---

## Comparison Questions

### How does Hexz compare to databases (PostgreSQL, SQLite)?

**Databases are for structured data** (tables, rows, columns):
- Complex query engine (WHERE, JOIN, GROUP BY)
- ACID transactions
- Schema enforcement

**Hexz is for blob data** (raw bytes, files):
- No schema — just store bytes
- No query engine — just seek and read
- Optimized for large sequential/random reads

**Use database when:** You need relational queries on structured data

**Use Hexz when:** You need fast access to large binary files

### How does Hexz compare to object storage (S3, MinIO)?

**S3 is for object storage** (files as API objects):
- HTTP API (GET/PUT/DELETE)
- Each file is a separate object
- Scales to billions of objects

**Hexz is for packaged storage** (files inside compressed archive):
- Single archive contains thousands of files
- Random access within archive
- Fewer S3 objects (lower cost, faster listings)

**Hybrid approach:**
```
S3: Store Hexz archive (one object)
  └─ Hexz: Contains 1M images (internal index)
```

**Benefits:**
- S3: Scale and durability
- Hexz: Compression and random access

### How does Hexz compare to Parquet for ML data?

**Parquet is for tabular data** (DataFrames):
- Columnar format (efficient for analytics)
- Schema required (strongly typed columns)
- Optimized for filtering/aggregation

**Hexz is for blob/file data** (images, videos, text):
- Row-oriented (efficient for fetching full samples)
- Schema-free (store arbitrary bytes)
- Optimized for sequential/random access

**Example dataset:**

| Data Type | Format | Rationale |
|-----------|--------|-----------|
| Tabular (CSVs, logs) | Parquet | Structured, column queries |
| Images (ImageNet) | Hexz | Blobs, random access |
| Mixed (image + metadata) | Both | Parquet for labels, Hexz for pixels |

---

## Operational Questions

### How do I update a dataset without repacking everything?

**Short answer:** Both fixed-size blocks and CDC support deduplication, but CDC is more resilient to changes.

**The difference:**

Both methods deduplicate identical blocks, but they differ in handling **edits and insertions**:

| Change Type | Fixed-Size Dedup | CDC Dedup |
|-------------|------------------|-----------|
| Append new data | Excellent (90%+) | Excellent (90%+) |
| Insert at start/middle | Poor (0-10%) | Excellent (90%+) |
| Edit existing data | Poor (0-10%) | Good (varies) |

**Why the difference?** The "boundary shift problem."

When you insert 100 bytes at the start of a file:
- **Fixed-size blocks:** All block boundaries shift by 100 bytes → Every block is now different → No deduplication
- **CDC blocks:** Boundaries are content-defined → Only the block containing the insertion changes → Rest deduplicate

**Example 1: Append-only (both work well)**
```bash
# v1: 100GB dataset
hexz pack v1/ -o v1.hxz  # 18GB compressed

# v2: Add 5GB new images (no edits to v1 data)
hexz pack v1/ v2_new/ -o v2.hxz  # 18.9GB (0.9GB new)
# Both fixed-size AND CDC achieve ~95% dedup
```

**Example 2: With insertions/edits (CDC wins)**
```bash
# v1: 100GB dataset
hexz pack v1/ -o v1.hxz --cdc  # 18GB compressed

# v2: Same data but 1,000 images edited + 1,000 inserted
hexz pack v2/ -o v2.hxz --cdc  # 19GB (1GB changed, 17GB deduplicated)
# CDC: 94% dedup
# Fixed-size: ~20% dedup (boundary shifts break it)
```

**When to use each:**

- **Use fixed-size (default, faster):** If you only append new data
  - Adding new training samples
  - Extending datasets without modifying existing data
  - Pack speed: 4.9 GB/s

- **Use CDC (slower, more resilient):** If you edit or insert data
  - Fixing mislabeled images
  - Inserting samples into middle of dataset
  - Modifying existing files
  - Pack speed: 1.9 GB/s

**Planned feature:** Incremental packing (append to existing archive without repacking).

### What does "v1, v2, v3" mean in benchmarks and examples?

**v1, v2, v3** refer to **different versions of the same dataset** over time. This is common in ML workflows where you iteratively improve or expand your training data.

**Real-world example:**

```
Week 1: Collect 10,000 images → Pack as v1.hxz
Week 2: Add 1,000 new images → Pack as v2.hxz
Week 3: Add 2,000 more images → Pack as v3.hxz
```

**Scenario A: Append-only (both fixed-size and CDC work well):**

[TODO: VALIDATE - Requires multi-snapshot deduplication test. Current benchmark only tests single-file internal duplication.]

```
Week 1: v1.hxz = X GB (10,000 images)
Week 2: Pack v1 + v2_new together
  → v2.hxz = X + Y GB (Y GB new, X GB deduplicated)
Week 3: Pack v1 + v2_new + v3_new together
  → v3.hxz = X + Z GB (Z GB new, X GB deduplicated)

Expected: Both fixed-size and CDC achieve >90% deduplication on append-only
```

**Validated Result (single-file, 25% internal duplication):**
- Test: 50 MB file with 25% repeated blocks
- Fixed-size: 50.00 MB → 50.22 MB (0% deduplication in this test)
- CDC: 50.00 MB → 40.19 MB (19.6% deduplication)

See `cargo bench --bench dedup_efficiency` for details.

**Scenario B: With edits/insertions (CDC wins):**

**Validated Result (shifted data benchmark):**
- Test: 50 MB base + 50 MB shifted (1KB inserted at start) packed into one snapshot
- Fixed-size: Base 50.22 MB, Combined 100.43 MB (0% dedup, all blocks shifted)
- CDC: Base 50.22 MB, Combined 54.02 MB (92.4% dedup, boundaries re-sync)

```
Week 1: v1.hxz = 50 GB (10,000 images)
Week 2: Same 10,000 images but 500 edited + 1,000 new inserted
  Fixed-size: v2.hxz = ~100 GB (boundary shifts break dedup entirely)
  CDC: v2.hxz = ~54 GB (92%+ of shifted data deduplicated)
```

See `cargo bench --bench dedup_efficiency` for the full shifted data benchmark.

**How deduplication works:**

Both methods break data into chunks and hash them. When packing v2:
- Old data → Hash matches existing chunks → Deduplicated (reused)
- New data → Hash doesn't match → Written to archive

**The difference:**
- **Fixed-size:** Chunk boundaries at fixed byte offsets (0, 64KB, 128KB, ...)
  - Pro: Fast to compute (no hashing needed for boundaries)
  - Con: Inserting 1 byte at position 0 shifts all boundaries → Breaks dedup

- **CDC:** Chunk boundaries based on content (hash-based cut points)
  - Pro: Inserting bytes doesn't shift boundaries → Dedup still works
  - Con: Slower (must compute rolling hash to find boundaries)

**Important notes:**

1. **Each version is a complete archive**
   - v2.hxz contains all 11,000 images (not a diff)
   - v3.hxz contains all 13,000 images
   - You can delete v1 and v2, v3 still works independently

2. **Deduplication happens during packing**
   - The dedup tracker hashes each chunk: "chunk hash ABC123"
   - If hash exists in previous data: Reuse existing block (deduplicated)
   - If hash is new: Compress and write block
   - Result: v2 file only contains new/changed chunks
   - Works with BOTH fixed-size and CDC (CDC just more resilient)

3. **Currently single-archive dedup only**
   - Dedup works when packing v1, v2, v3 together in one operation
   - Cross-archive dedup (pack v2 while referencing v1.hxz) is planned

4. **Benchmark context**
   - When we say "v2 with 10% changes", we mean:
     - 90% of data unchanged from v1
     - 10% new or modified data
   - Both methods deduplicate the unchanged 90% IF changes are append-only
   - CDC deduplicates even if changes involve insertions/edits
   - Fixed-size dedup fails if insertions cause boundary shifts

**When versioning matters:**

- **Research iterations:** "ImageNet + 5% synthetic data" → v1 + v2
- **Continuous collection:** Weekly dataset updates
- **A/B testing:** Different augmentation strategies
- **Checkpoints:** Model weights at different epochs (highly redundant)

**When versioning doesn't matter:**

- **One-time datasets:** Pack once, never change
- **Completely different data:** v2 shares nothing with v1
- **Fast iteration:** Deleting old versions immediately

### How do I migrate from WebDataset to Hexz?

See [Migrate from WebDataset](how-to/ml-workflows/migrate-from-webdataset.md) for detailed guide.

**Quick version:**

```bash
# WebDataset structure:
# data/
#   shard_00000.tar
#   shard_00001.tar
#   ...
#   shard_09999.tar

# Convert to Hexz:
hexz pack data/ -o dataset.hxz --compression lz4

# PyTorch code change:
# OLD:
# dataset = WebDataset("data/shard_{00000..09999}.tar")

# NEW:
dataset = hexz.open("dataset.hxz")
```

**Benefits:**
- Single file instead of 10,000 shards
- True shuffling (not shard-limited)
- Faster random access

### How do I monitor Hexz performance?

**Built-in metrics:**
```python
import hexz

reader = hexz.open("data.hxz", enable_metrics=True)

# After training:
stats = reader.get_metrics()
print(f"Cache hit rate: {stats.cache_hits / stats.total_reads:.1%}")
print(f"Bytes read: {stats.bytes_read / 1e9:.2f} GB")
print(f"Decompression time: {stats.decompress_time_ms:.0f} ms")
```

**Integration with existing tools:**
- Prometheus: Export metrics via `/metrics` endpoint (server mode)
- TensorBoard: Log throughput during training
- CloudWatch: S3 bandwidth monitoring (range request counts)

### What happens if a block is corrupted?

**Detection:**
- Each block has BLAKE3 hash in index
- Corruption detected on read (hash mismatch)

**Recovery:**
```bash
# Verify archive integrity
hexz verify dataset.hxz

# Output:
# Block 1523: CORRUPTED (hash mismatch)
# Blocks scanned: 10000
# Errors: 1

# Attempt repair (if redundancy enabled - future feature)
hexz repair dataset.hxz
```

**Current state:** Corruption detection works, automatic repair not yet implemented.

**Mitigation:**
- Store on reliable storage (S3 with versioning, RAID)
- Use `--verify` flag during pack to ensure integrity
- Keep backups of critical datasets

---

## Design Philosophy Questions

### Why Rust instead of C++ or Python?

**Rust advantages:**
- **Memory safety** without garbage collection (no GC pauses)
- **Fearless concurrency** (data race prevention at compile time)
- **Zero-cost abstractions** (high-level code, low-level performance)
- **Cross-platform** (excellent Windows/macOS/Linux support)
- **Python interop** (PyO3 bindings with minimal overhead)

See [ADR-0001: Rust for Core Engine](adr/0001-rust-for-core-engine.md) for full rationale.

### Why block-level compression instead of file-level?

**File-level compression** (tar.gz, 7z):
- YES Best compression ratio (uses entire file context)
- NO No random access (must decompress from start)

**Block-level compression** (Hexz):
- YES Random access (decompress only needed blocks)
- NO ~15-20% worse compression ratio

**Trade-off justification:**

For a 1TB dataset:
- File-level: 200GB compressed, **5 seconds** to read random sample
- Block-level: 250GB compressed, **6 µs** to read random sample

**We trade 50GB extra space for 833,000× faster random access.**

For ML training with shuffling, this is the right trade-off.

### Why not use existing formats (Avro, ORC, Arrow)?

**Avro/ORC are for tabular data:**
- Require schema definition
- Optimized for columnar analytics
- Complex to use for blob data (images, videos)

**Arrow is for in-memory data:**
- Not a storage format (though IPC exists)
- No compression strategy for disk
- Designed for zero-copy between processes, not long-term storage

**Hexz is purpose-built for:**
- Schema-free blob storage
- Random access to compressed data
- Seekable streaming from remote storage

If your data is tabular, use Parquet. If it's blobs, use Hexz.

### Why allow disabling CDC? Isn't deduplication always good?

**No! Deduplication has costs:**

**CDC overhead:**
- 2.6× slower packing (4.9 GB/s → 1.9 GB/s)
- More complex index (variable-sized chunks)
- Slightly higher CPU usage during reads (random chunk boundaries)

**When CDC is not worth it:**

| Scenario | Dedup Savings | CDC Overhead | Verdict |
|----------|--------------|-------------|---------|
| Random/unique data | ~0% | 2.6× slower | NO Skip CDC |
| First version of dataset | ~0% | 2.6× slower | NO Skip CDC |
| Highly compressed data | <5% | 2.6× slower | NO Skip CDC |
| ML training (hot data) | N/A (read-only) | Slightly slower reads | NO Skip CDC |

**When CDC is worth it:**

| Scenario | Dedup Savings | CDC Overhead | Verdict |
|----------|--------------|-------------|---------|
| Dataset v1 → v2 (5% change) | 95% | 2.6× slower (one-time) | YES Use CDC |
| VM snapshots | 60-80% | Acceptable | YES Use CDC |
| Multiple related datasets | 30-50% | One-time cost | YES Use CDC |

**Default:** CDC disabled (opt-in with `--cdc` flag).

---

## Troubleshooting

### Why is my first epoch slow on S3?

**Expected behavior!** First epoch must download data from S3:

- **First epoch:** 100-200 MB/s (network-bound from S3)
- **Second epoch:** 5-9 GB/s (cached in RAM)

**Optimizations:**
1. **Prefetch:** Use `--prefetch=2` to overlap network and compute
2. **Larger cache:** Increase `--cache-size` to keep more blocks in RAM
3. **Faster S3 region:** Co-locate compute and storage (us-east-1 → us-east-1)
4. **S3 Transfer Acceleration:** Enable for global access

### Why is packing slower than I expected?

**Common causes:**

1. **CDC enabled on random data:**
   - Rolling hash overhead with no dedup benefit
   - Solution: Disable CDC with `--no-cdc`

2. **Slow compression algorithm:**
   - Zstd-9 is 4.7× slower than Zstd-3
   - Solution: Use LZ4 for speed, Zstd-3 for balance

3. **Disk bottleneck:**
   - Slow HDD vs fast NVMe
   - Check with: `dd if=/dev/zero of=test bs=1M count=1000`

4. **Small block size:**
   - 4KB blocks have high metadata overhead
   - Solution: Use default 64KB blocks

### Cache is not helping — why?

**Possible issues:**

1. **Cache too small for working set:**
   ```python
   # Working set: 2GB (30% of 10GB dataset)
   # Cache: 256MB (default)
   # Hit rate: 12% (too small!)

   # Solution: Increase cache
   reader = hexz.open("data.hxz", cache_size_mb=4096)
   ```

2. **Access pattern too random:**
   - True random access defeats caching
   - Solution: Use batch prefetching, sequential scan when possible

3. **Multiple workers with separate caches:**
   - 8 workers × 256MB = 2GB total, but duplicated blocks
   - Solution: Shared cache (planned feature) or fewer workers

### How do I debug slow training?

**Step 1: Profile where time is spent:**

```python
import time
import hexz

reader = hexz.open("data.hxz")

# Measure pure I/O
start = time.time()
for i in range(1000):
    data = reader.read_sample(i)
io_time = time.time() - start
print(f"I/O: {io_time:.2f}s ({1000/io_time:.0f} samples/sec)")

# Measure I/O + transforms
start = time.time()
for i in range(1000):
    data = reader.read_sample(i)
    img = transform(data)  # Your preprocessing
total_time = time.time() - start
print(f"Total: {total_time:.2f}s ({1000/total_time:.0f} samples/sec)")

# Compare:
# If I/O is slow → Hexz bottleneck (increase cache, prefetch)
# If transform is slow → CPU bottleneck (use GPU preprocessing)
```

**Step 2: Check system resources:**

```bash
# CPU usage (should be near 100% on all cores)
htop

# Disk I/O (should be low after first epoch)
iotop

# Network I/O (S3 bandwidth)
iftop
```

See [Performance Tuning Guide](how-to/performance-tuning.md) for detailed optimization.

---

## Future Roadmap Questions

### Will Hexz support streaming writes?

**Planned:** Yes, incremental packing is on the roadmap.

**Current state:** Must pack entire dataset at once.

**Workaround:** Pack new data separately, merge later:
```bash
hexz pack new_data/ -o delta.hxz --cdc
hexz merge base.hxz delta.hxz -o combined.hxz
```

### Will Hexz support cross-snapshot deduplication?

**Parent-child deduplication already works** via thin snapshots — a child snapshot only stores blocks that differ from its parent (see above).

**Planned:** A shared external deduplication index for unrelated snapshots:
```bash
# Pack with global dedup index
hexz pack v1/ -o v1.hxz --dedup-index global.idx
hexz pack v2/ -o v2.hxz --dedup-index global.idx  # Reuses blocks from v1
```

**ETA:** Tentatively v0.3.0 (see [ROADMAP.md](project-docs/ROADMAP.md))

### Will Hexz support GPU decompression?

**Investigating:** GPU decompression for LZ4 exists (nvCOMP library).

**Challenges:**
- PCIe transfer overhead may negate benefits
- Works well for large batches (>1MB), not small blocks (64KB)

**Planned:** Prototype in v0.4.0, evaluate speedup on real workloads.

---

## Getting Help

**Still have questions?**

-  [Read the full documentation](index.md)
-  [Join GitHub Discussions](https://github.com/Alethic-Systems/hexz/discussions)
-  [Report an issue](https://github.com/Alethic-Systems/hexz/issues)
-  Contact: [support@hexz.dev](mailto:support@hexz.dev)
