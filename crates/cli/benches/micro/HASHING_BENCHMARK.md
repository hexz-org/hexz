# Hashing Performance Benchmark: BLAKE3 vs SHA-256

This benchmark validates the performance improvement from switching to BLAKE3 for content-defined chunking (CDC) and deduplication in commit `6416b8d`.

## Benchmark Results

### Raw Hashing Throughput

| Block Size | BLAKE3 | SHA-256 | Speedup |
|------------|--------|---------|---------|
| 4 KB       | 3.98 GiB/s | 2.42 GiB/s | **1.64×** |
| 64 KB      | 5.30 GiB/s | 2.45 GiB/s | **2.16×** |
| 256 KB     | 5.35 GiB/s | 2.45 GiB/s | **2.18×** |
| 1 MB       | 5.33 GiB/s | 2.46 GiB/s | **2.17×** |

### Real-World Deduplication Workflow

Testing 100 blocks of 64KB each with realistic duplicate patterns:

| Hash Function | Throughput | Speedup |
|---------------|------------|---------|
| BLAKE3        | 53.5 MiB/s | **2.14×** |
| SHA-256       | 25.0 MiB/s | 1.00× |

The deduplication workflow includes:
1. Hashing compressed block data
2. HashMap lookup for existing blocks
3. Inserting new unique blocks into the hash table

## Analysis

### Performance Characteristics

- **BLAKE3** consistently delivers **2.1-2.2× faster** hashing than SHA-256 for typical chunk sizes (64KB-1MB)
- For small 4KB blocks, BLAKE3 is still **1.64× faster**
- The speedup holds true in the **complete deduplication workflow**, not just raw hashing
- Both algorithms show stable performance across different block sizes

### Why Not 6× as Claimed?

The commit message claimed "3200 MB/s vs. SHA-256 at 500MB/s" (6.4× speedup), but our benchmark shows 2.1-2.2× speedup. The difference is likely due to:

1. **Hardware differences**: The original claim may have been from different hardware with different SIMD capabilities
2. **Real-world conditions**: Our benchmark uses realistic data patterns, not ideal test vectors
3. **Measurement methodology**: The original numbers may have been from dedicated hash benchmarks under optimal conditions

### Still a Significant Win

Even at 2.1× speedup, this is a **major performance improvement** for deduplication workloads:

- **Deduplication overhead reduced by 53%** (from 25 MiB/s to 53.5 MiB/s)
- For a 1GB deduplication workload: **saves ~20 seconds** of hashing time
- BLAKE3 maintains the same **128-bit collision resistance** as SHA-256
- BLAKE3 is a **modern, well-studied cryptographic hash** with excellent security properties

## Security Considerations

Both BLAKE3 and SHA-256 provide:
- 256-bit hash output (using 32 bytes)
- 128-bit collision resistance (astronomically unlikely at 2^128 blocks)
- Cryptographically secure for content addressing

BLAKE3 advantages:
- Faster performance through SIMD optimization
- Modern design (2020) vs SHA-256 (2001)
- Parallelizable tree structure

## Running the Benchmark

```bash
# Run hashing benchmark only
make bench hashing

# Run and save as baseline
make save-baseline blake3-switch

# Archive for later comparison
make archive-baseline blake3-switch
```

## Conclusion

The switch from SHA-256 to BLAKE3 provides a **proven 2.1× performance improvement** in real-world deduplication scenarios, reducing the computational overhead of content-addressed storage while maintaining equivalent security properties.
