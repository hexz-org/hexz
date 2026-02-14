# Hexz Performance Benchmarks

This document contains **validated** performance measurements for Hexz. All benchmarks are reproducible using the commands listed below.

**Test System:**
- **CPU**: Intel i7-14700K (8P+12E cores, 20 cores total)
- **RAM**: 64GB DDR4
- **Storage**: NVMe SSD
- **OS**: Linux (Arch)
- **Rust**: Release mode with optimizations

**Last Updated**: 2026-02-14

---

## Quick Reference

| Metric | Value | Benchmark Command |
|--------|-------|-------------------|
| **LZ4 Compress** | 23.6 GB/s | `cargo bench --bench compression` |
| **LZ4 Decompress** | 32.1 GB/s | `cargo bench --bench compression` |
| **Zstd-3 Compress** | 9.0 GB/s | `cargo bench --bench compression` |
| **Zstd-3 Decompress** | 13.4 GB/s | `cargo bench --bench compression` |
| **Zstd-9 Compress** | 1.9 GB/s | `cargo bench --bench compression` |
| **Zstd-9 Decompress** | 13.4 GB/s | `cargo bench --bench compression` |
| **BLAKE3 Hash** | 5.4 GB/s | `cargo bench --bench hashing` |
| **SHA-256 Hash** | 2.5 GB/s | `cargo bench --bench hashing` |
| **FastCDC Chunking** | 2.7 GB/s | `cargo bench --bench cdc_chunking` |
| **AES-256-GCM Encrypt** | 2.1 GB/s | `cargo bench --bench encryption` |
| **AES-256-GCM Decrypt** | 2.1 GB/s | `cargo bench --bench encryption` |
| **Pack LZ4 (no CDC)** | 4.9 GB/s | `cargo bench --bench write_throughput` |
| **Pack Zstd-3 (no CDC)** | 1.6 GB/s | `cargo bench --bench write_throughput` |
| **Pack LZ4 + CDC** | 1.9 GB/s | `cargo bench --bench write_throughput` |
| **Pack 64KB blocks** | 4.7 GB/s | `cargo bench --bench block_size_tradeoffs` |
| **Pack 256KB blocks** | 5.0 GB/s | `cargo bench --bench block_size_tradeoffs` |
| **Sequential Read (100MB)** | 9.0 GB/s | `cargo bench --bench read_throughput` |
| **Sequential Read (500MB)** | 7.8 GB/s | `cargo bench --bench read_throughput` |
| **Random Access (cold)** | 6.6 µs | `cargo bench --bench sparse_access` |
| **Random Access (warm)** | 174 ns | `cargo bench --bench sparse_access` |
| **Concurrent Read (4 threads)** | 13.1 GB/s | `cargo bench --bench concurrency` |

---

## Compression Performance

### Algorithm Comparison

**Benchmark:** `cargo bench --bench compression`

Measured on 1MB deterministic test data (single-threaded):

| Algorithm | Compress | Decompress | Best For |
|-----------|----------|------------|----------|
| **LZ4** | 23.3 GB/s | 32.1 GB/s | Hot data, AI training, low-latency access |
| **Zstd-3** | 9.1 GB/s | 13.5 GB/s | Balanced compression/speed for storage |
| **Zstd-9** | 1.9 GB/s | 13.4 GB/s | Maximum compression ratio for archival |

**Notes:**
- LZ4 decompression is 1.4× faster than compression
- Zstd-3 decompression is 1.5× faster than compression
- Zstd-9 compress is 4.7× slower than Zstd-3, but decompress speed is identical
- All algorithms show excellent single-threaded performance on modern CPUs
- Compression ratios not yet validated on real-world datasets

**Recommendation:** Use LZ4 for datasets accessed frequently during training. Use Zstd-3 for cold storage with good balance. Use Zstd-9 for maximum compression when pack time is not critical.

---

## Hashing Performance

### BLAKE3 vs SHA-256

**Benchmark:** `cargo bench --bench hashing`

Measured on various block sizes:

| Block Size | BLAKE3 | SHA-256 | Speedup |
|------------|--------|---------|---------|
| 4KB | 4.0 GB/s | 2.4 GB/s | 1.7× |
| 64KB | 5.4 GB/s | 2.5 GB/s | 2.2× |
| 256KB | 5.4 GB/s | 2.5 GB/s | 2.2× |
| 1MB | 5.3 GB/s | 2.5 GB/s | 2.1× |

**Notes:**
- BLAKE3 is 1.7-2.2× faster than SHA-256 across block sizes
- Best performance at 64KB+ block sizes (5.4 GB/s)
- Smaller 4KB blocks have higher overhead (4.0 GB/s)
- Both use hardware acceleration where available

**Deduplication Workflow Performance:**

Testing 100 blocks (64KB each) with hash table lookups:
- BLAKE3 workflow: 54.0 MB/s throughput
- SHA-256 workflow: 25.3 MB/s throughput
- BLAKE3 is 2.1× faster in real deduplication scenarios

---

## FastCDC Chunking Performance

**Benchmark:** `cargo bench --bench cdc_chunking`

### Throughput by Data Pattern (10MB test, 16KB avg chunks)

| Pattern | Throughput | Notes |
|---------|------------|-------|
| Random | 2.7 GB/s | Baseline performance |
| Compressible | 2.8 GB/s | Slightly faster (cache-friendly) |
| Zeros | 2.7 GB/s | Similar to random |
| Repeated | 2.5 GB/s | Slightly slower |
| Fixed-size baseline | 26 GB/s | 9.6× faster (just slicing, no hashing) |

### Throughput by Chunk Size

| Average Chunk Size | Throughput |
|--------------------|------------|
| 8KB | 2.7 GB/s |
| 16KB | 2.7 GB/s |
| 32KB | 2.7 GB/s |

**Key Findings:**
- FastCDC achieves **2.7 GB/s** throughput (5.4× faster than previously claimed "~500 MB/s")
- Performance is consistent across different chunk sizes and data patterns
- CDC overhead is ~10× vs fixed-size chunking, but acceptable for deduplication benefits
- Not bottlenecked by data pattern or chunk size configuration

**Recommendation:** CDC overhead is acceptable for scenarios where deduplication provides >10% space savings.

---

## Encryption Performance

**Benchmark:** `cargo bench --bench encryption`

### AES-256-GCM Throughput

| Block Size | Encrypt | Decrypt | Notes |
|------------|---------|---------|-------|
| 4KB | 2.1 GB/s | 2.1 GB/s | Small block overhead |
| 16KB | 2.1 GB/s | 2.1 GB/s | Typical image size |
| 64KB | 2.1 GB/s | 2.1 GB/s | Default block size |
| 256KB | 2.1 GB/s | 2.1 GB/s | Large block |
| 1MB | 2.1 GB/s | 2.1 GB/s | Maximum efficiency |

**Key Findings:**
- Encryption/decryption throughput is **2.1 GB/s** (within claimed "1-2 GB/s" range)
- Performance is symmetric between encrypt and decrypt operations
- Throughput is consistent across all block sizes (AES-NI hardware acceleration working well)
- Roundtrip (encrypt + decrypt) throughput: 1.0 GB/s for 64KB blocks

**Notes:**
- AES-NI hardware acceleration confirmed active
- 128-bit authentication tag adds 16 bytes overhead per block
- Encryption disables deduplication (each block has unique nonce)

**Recommendation:** Use encryption for sensitive data on untrusted storage. Accept ~2× throughput reduction vs unencrypted.

---

## Write Throughput Performance

**Benchmark:** `cargo bench --bench write_throughput`

### Pack Operation Throughput (100MB test files)

| Configuration | Throughput | Notes |
|--------------|------------|-------|
| LZ4 (no CDC) | 4.9 GB/s | Fast baseline |
| Zstd-3 (no CDC) | 1.6 GB/s | Better compression, slower |
| LZ4 + CDC | 1.9 GB/s | CDC adds 2.6× overhead |

**Key Findings:**
- LZ4 pack throughput: **4.9 GB/s** (excellent for hot data)
- Zstd-3 pack throughput: **1.6 GB/s** (good balance for cold storage)
- **CDC overhead:** Reduces throughput by 2.6× (4.9 GB/s → 1.9 GB/s)
- Bottleneck: CDC chunking (2.7 GB/s) limits overall pack performance

**Analysis:**
- Without CDC: Compression is the bottleneck
- With CDC: FastCDC becomes the bottleneck (2.7 GB/s chunking < 4.9 GB/s LZ4 compression)
- For 100MB file: Pack time = 20ms (LZ4) vs 50ms (LZ4+CDC) vs 60ms (Zstd-3)

**Recommendation:**
- Use LZ4 without CDC for maximum pack speed (4.9 GB/s)
  - Fixed-size blocks still deduplicate append-only updates
- Use LZ4 with CDC when data has insertions/edits (1.9 GB/s)
  - Resilient to boundary shifts, better dedup on modified data
- Use Zstd-3 for archival/cold storage where compression ratio matters

**Note**: CDC only affects PACKING speed. Once packed, both CDC and fixed-size archives READ at the same 9.0 GB/s during training.

---

## Block Size Tradeoffs

**Benchmark:** `cargo bench --bench block_size_tradeoffs`

### Pack Throughput by Block Size (100MB test files, LZ4)

| Block Size | Pack Throughput | Notes |
|------------|-----------------|-------|
| 4KB | 2.4 GiB/s | High metadata overhead |
| 16KB | 3.6 GiB/s | Good balance |
| 64KB | 4.8 GiB/s | Default, optimal for most workloads |
| 256KB | 5.2 GiB/s | Large blocks, less metadata |
| 1MB | 5.0 GiB/s | Maximum throughput, huge blocks |

**Key Findings:**
- Smaller blocks (4KB) have higher metadata overhead, reducing pack throughput by ~50%
- 64KB default block size provides excellent balance
- Diminishing returns beyond 256KB
- Block size primarily affects pack time, not compression ratio for this data pattern

**Recommendation:**
- Use 64KB for general workloads (default)
- Use 256KB-1MB for maximum pack speed if metadata overhead matters
- Use 16KB-64KB for better random access granularity

---

## Random Access Performance

### Access Latency

**Benchmark:** `cargo bench --bench sparse_access`

| Access Pattern | Latency | Notes |
|----------------|---------|-------|
| Cold cache (4KB) | 6.6 µs | Block decompression required |
| Warm cache (4KB) | 174 ns | Data already in memory |

**Speedup:** Warm cache is **38× faster** than cold cache.

**Notes:**
- Cold cache performance includes decompression overhead
- Warm cache is limited only by memory access speed
- Both measurements are for LZ4-compressed data

---

## Sequential Read Performance

### Throughput Tests

**Benchmark:** `cargo bench --bench read_throughput`

| Snapshot Size | Throughput | Notes |
|--------------|------------|-------|
| 100 MiB | 9.0 GB/s | Single-threaded sequential read |
| 500 MiB | 7.8 GB/s | Sustained performance on larger file |

**Notes:**
- Both tests use LZ4 compression
- Single-threaded performance
- Slight degradation on larger files likely due to cache effects

---

## Concurrent Access Performance

### Multi-threaded Reading

**Benchmark:** `cargo bench --bench concurrency`

| Configuration | Throughput | Speedup vs Single-threaded |
|--------------|------------|---------------------------|
| 4 threads, 50MB each | 13.1 GB/s | ~1.5× |

**Notes:**
- Test uses 4 threads reading different 50MB regions concurrently
- Near-linear scaling up to number of P-cores (8 on i7-14700K)
- Demonstrates thread-safe concurrent access capability

---

## Multi-worker Scaling

### Parallel Data Loading

**Benchmark:** `cargo bench --bench ai_multiworker`

| Workers | Throughput | Speedup vs 1 Worker |
|---------|------------|---------------------|
| 1 | 2.6 GB/s | 1.0× (baseline) |
| 2 | 3.3 GB/s | 1.3× |
| 4 | 4.9 GB/s | 1.9× |
| 8 | 7.5 GB/s | 2.9× |
| 16 | 10.5 GB/s | 4.1× |

**Notes:**
- Simulates PyTorch DataLoader worker pattern (Rust-level parallelism)
- Scaling limited by CPU architecture (8 P-cores + 12 E-cores)
- Best performance with 8-16 workers on this CPU
- Real PyTorch integration scaling not yet validated

---

## AI/ML DataLoader Performance

### Simulated DataLoader Patterns

**Benchmark:** `cargo bench --bench ai_dataloader`

#### Sequential vs Random Access

| Access Pattern | Throughput | Notes |
|----------------|------------|-------|
| Sequential (10K samples) | 6.0 GB/s | Predictable access, good prefetch |
| Random (10K samples) | 5.9 GB/s | Shuffled access, minimal degradation |

**Notes:**
- Minimal performance difference between sequential and random access
- Shows effectiveness of block-level caching

#### Batch Size Impact

| Batch Size | Throughput | Notes |
|------------|------------|-------|
| 1 | 5.8 GB/s | Per-sample overhead |
| 16 | 7.2 GB/s | Good balance |
| 32 | 8.0 GB/s | Amortized overhead |
| 64 | 8.3 GB/s | Approaching peak |
| 128 | 10.5 GB/s | Maximum throughput |

**Recommendation:** Batch size of 32-64 provides good balance between latency and throughput.

#### Sample Size Scaling

| Sample Size | Throughput | Notes |
|-------------|------------|-------|
| 1 KB | 2.6 GB/s | Small samples, overhead dominant |
| 4 KB | 7.6 GB/s | Typical small images/text |
| 16 KB | 17.9 GB/s | Medium-sized samples |
| 64 KB | 16.2 GB/s | Large samples (e.g., images) |
| 256 KB | 16.3 GB/s | Very large samples |
| 1 MB | 14.7 GB/s | Huge samples, I/O bound |

**Notes:**
- Best throughput with 16-64KB samples
- Very small samples have overhead from frequent function calls
- Very large samples may hit I/O bandwidth limits

#### Cache Effectiveness

| Cache State | Throughput | Notes |
|-------------|------------|-------|
| Cold cache | 1.8 GB/s | First access, decompression required |
| Warm cache | 7.8 GB/s | 4.3× faster on repeated access |

**Impact:** Caching provides massive speedup for repeated data access (e.g., multiple epochs).

---

## Shuffling Performance

**Benchmark:** `cargo bench --bench ai_shuffle`

### Shuffle Algorithm Scaling

Fisher-Yates shuffle performance:

| Dataset Size | Time | Throughput |
|--------------|------|------------|
| 1K samples | ~2 µs | 500 M samples/s |
| 10K samples | ~20 µs | 500 M samples/s |
| 100K samples | ~200 µs | 500 M samples/s |
| 1M samples | ~2 ms | 500 M samples/s |

**Notes:**
- Linear scaling with dataset size (O(n) algorithm)
- Consistent throughput across all sizes
- Shuffling is negligible overhead compared to data loading

### PRNG Comparison

Different random number generators for shuffling:

| PRNG | Time (1M samples) | Notes |
|------|------------------|-------|
| Xorshift64 | 2.07 ms | Fast, good distribution |
| SimpleModulo | 2.03 ms | Fastest, slightly biased |
| Xorshift128 | 2.05 ms | Better quality, minimal overhead |

**Recommendation:** All PRNGs perform similarly; use Xorshift64 for balance of speed and quality.

---

## Cache Concurrency

**Benchmark:** `cargo bench --bench cache_concurrent`

### Concurrent Cache Access

| Threads | Hit Rate | Throughput | Notes |
|---------|----------|------------|-------|
| 2 | High | 366 K ops/s | Good scaling |
| 4 | High | 952 K ops/s | Near-linear |
| 8 | High | 1.78 M ops/s | Excellent scaling |

**Cache Miss (concurrent insertion):**

| Threads | Throughput | Notes |
|---------|------------|-------|
| 2 | 39 K ops/s | Contention on writes |
| 4 | 49 K ops/s | Lock contention visible |
| 8 | 60 K ops/s | Scaling limited by locks |

**Notes:**
- Read-heavy workloads scale very well (near-linear to 8 threads)
- Write-heavy workloads show lock contention
- Typical ML workloads are read-heavy, so this is acceptable

---

## Deduplication Efficiency

**Benchmark:** `cargo bench --bench dedup_efficiency`

This benchmark measures actual compression ratios and deduplication percentages on controlled datasets with known duplication patterns.

### No Duplication (Random Data)

Testing 50 MB of purely random data (no repeated blocks):

| Method | Input Size | Output Size | Compression Ratio | Space Savings |
|--------|-----------|------------|-------------------|---------------|
| Fixed-size blocks | 50.00 MB | 50.22 MB | 1.00x | -0.4% |
| CDC blocks | 50.00 MB | 50.22 MB | 1.00x | -0.4% |

**Analysis:**
- Random data is incompressible with LZ4
- Both methods produce similar output (no deduplication possible)
- Slight size increase (-0.4%) is metadata overhead from the archive format
- This establishes baseline: neither method adds significant overhead on incompressible data

### 25% Duplication

Testing 50 MB with 25% repeated blocks (same blocks appear multiple times):

| Method | Input Size | Output Size | Compression Ratio | Space Savings |
|--------|-----------|------------|-------------------|---------------|
| Fixed-size blocks | 50.00 MB | 50.22 MB | 1.00x | -0.4% |
| CDC blocks | 50.00 MB | 40.19 MB | 1.24x | 19.6% |

**Analysis:**
- **CDC achieved 19.6% space savings** by detecting and deduplicating repeated blocks
- Fixed-size blocks showed no deduplication in this test (0% savings)
- **CDC difference: 20.0% smaller** than fixed-size output
- This demonstrates CDC's ability to detect duplicate content within a single file

**Note:** The fixed-size result showing no deduplication needs further investigation. This may be due to:
1. Test data pattern not aligning with fixed block boundaries
2. Deduplication logic requiring multiple files/snapshots rather than internal duplication
3. Implementation detail in how fixed-size chunking handles single-file packing

### Shifted Data (Boundary Shift Problem)

Testing base (50 MB) + shifted (50 MB + 1KB insertion at start) packed into ONE snapshot.
Both streams share a dedup map, enabling cross-file deduplication.

| Method | Base Only | Base + Shifted | Shifted Overhead | Dedup of Shifted |
|--------|----------|---------------|-----------------|-----------------|
| Fixed-size blocks | 50.22 MB | 100.43 MB | 50.22 MB | -0.4% |
| CDC blocks | 50.22 MB | 54.02 MB | 3.81 MB | 92.4% |

**CDC advantage: 92.8 percentage points better deduplication on shifted data.**

**Analysis:**
- **Fixed-size**: 1KB insertion shifts every block boundary. All ~763 blocks appear "new" despite
  containing the same data. Zero deduplication. Combined output = 2x base (100.43 MB).
- **CDC**: Content-defined boundaries re-sync after the insertion point. Only ~2-3 blocks near the
  insertion differ. 92.4% of shifted data deduplicated against base. Combined output = base + 3.81 MB.

**Why this matters for ML:**
- Dataset v1 -> v2 often involves inserting new samples (not just appending)
- Fixed-size chunking breaks deduplication when data shifts
- CDC maintains deduplication across versions with insertions/edits
- The 2.6x slower pack speed of CDC pays for itself when it avoids storing 50 MB of duplicate data

---

## Reproducing Benchmarks

### Run All Benchmarks

```bash
# From repository root

# Micro-benchmarks
cargo bench --bench hashing          # Hash algorithm comparison
cargo bench --bench compression      # Compression throughput
cargo bench --bench cache_concurrent # Cache concurrency

# Macro-benchmarks
cargo bench --bench read_throughput  # Sequential read
cargo bench --bench sparse_access    # Random access latency
cargo bench --bench concurrency      # Multi-threaded access

# AI/ML benchmarks
cargo bench --bench ai_dataloader    # DataLoader patterns
cargo bench --bench ai_shuffle       # Shuffling performance
cargo bench --bench ai_multiworker   # Worker scaling
cargo bench --bench ai_prefetch      # Prefetch strategies
cargo bench --bench ai_tensor_ops    # Tensor operations
cargo bench --bench ai_ml_workloads  # End-to-end workflows
```

### Run Specific Benchmark

```bash
# Run one specific test
cargo bench --bench compression -- "LZ4 Compress"

# Run with specific parameters
cargo bench --bench ai_multiworker -- "workers/8"

# Save baseline for comparison
cargo bench --bench compression -- --save-baseline i7_14700k

# Compare against baseline
cargo bench --bench compression -- --baseline i7_14700k
```

### View Results

```bash
# Open HTML report
open target/criterion/compression/report/index.html

# Or for specific test
open target/criterion/Compression/LZ4_Compress/report/index.html
```
