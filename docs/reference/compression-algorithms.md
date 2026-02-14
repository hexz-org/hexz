# Compression Algorithms

Comparison of compression algorithms supported by Hexz.

## Supported Algorithms

Hexz supports two compression algorithms optimized for different use cases.

## LZ4

**Type**: Fast compression/decompression
**Use Case**: Hot data, frequent random access, local NVMe storage

### Characteristics

- **Compression Speed**: Very fast
- **Decompression Speed**: Very fast
- **Compression Ratio**: Moderate (2-3x typical)
- **CPU Usage**: Low
- **Memory Usage**: Low

### When to Use LZ4

- Random access workloads (ML training with shuffling)
- VM boot scenarios requiring low latency
- Local NVMe storage where I/O is fast
- CPU-constrained environments
- Real-time data processing

### Configuration

```bash
# CLI
hexz data pack --disk data/ --output out.hxz --compression lz4
```

```python
# Python
hexz.open("out.hxz", mode="w", compression="lz4")
```

## Zstandard (Zstd)

**Type**: High compression ratio
**Use Case**: Cold storage, S3 streaming, archival

### Characteristics

- **Compression Speed**: Moderate to slow (depending on level)
- **Decompression Speed**: Fast to moderate
- **Compression Ratio**: High (3-5x typical, up to 8x at level 22)
- **CPU Usage**: Medium to high
- **Memory Usage**: Medium

### Compression Levels

| Level | Speed | Ratio | Use Case |
|-------|-------|-------|----------|
| 1 | Fastest | Lowest | Real-time compression |
| 3 | Fast | Good | Default, balanced |
| 9 | Moderate | Better | S3 storage |
| 15 | Slow | High | Archival |
| 22 | Very slow | Highest | Long-term cold storage |

### When to Use Zstandard

- S3 streaming (save bandwidth)
- Archival storage
- Sequential access patterns
- Storage-constrained environments
- Data with high redundancy

### Configuration

```bash
# CLI with compression level
hexz data pack \
  --disk data/ \
  --output out.hxz \
  --compression zstd \
  --compression-level 9
```

```python
# Python with compression level
hexz.open("out.hxz", mode="w", compression="zstd", compression_level=9)
```

## Comparison

### Performance

Validated on i7-14700K (single-threaded):

| Algorithm | Compress | Decompress | Ratio | CPU |
|-----------|----------|------------|-------|-----|
| LZ4 | 22 GB/s | 32 GB/s | 2-3x [measured in benchmarks] | Low |
| Zstd-3 | 8-9 GB/s | [not measured] | 3-4x [estimated] | Medium |
| Zstd-9+ | [not measured] | [not measured] | [not measured] | High |

Note: Actual performance varies by data characteristics. Compression ratios need validation with real datasets.

### Storage Savings

[BENCHMARK NOT YET VALIDATED]

Compression ratios depend heavily on data characteristics. Typical ranges:
- LZ4: 2-3× (measured in micro-benchmarks)
- Zstd-3: 3-4× (estimated)
- Zstd-9+: 4-5× (estimated)

Actual savings on real datasets (images, VMs, code) need validation through macro-benchmarks.

### Use Case Recommendations

| Scenario | Algorithm | Level | Reason |
|----------|-----------|-------|--------|
| ML training (local) | LZ4 | N/A | Fast decompression, random access |
| ML training (S3) | Zstd | 3-9 | Balance bandwidth and decompression |
| VM boot | LZ4 | N/A | Low latency required |
| Archival | Zstd | 15-22 | Maximum compression |
| Frequent updates | LZ4 | N/A | Faster repack operations |
| Infrequent access | Zstd | 9+ | Optimize storage cost |

## Block Size Impact

[BENCHMARK NOT YET VALIDATED]

Compression ratio generally improves with larger blocks, but this trades off against random access latency. A dedicated benchmark is needed to measure the actual relationship between block size, compression ratio, and access latency.

Default block size is 64KB as a balanced compromise. See [ADR-0002](../adr/0002-block-level-compression.md) for block-level compression rationale.

## Choosing an Algorithm

**Decision tree**:

1. **Is storage/bandwidth the primary constraint?**
   - Yes: Use Zstd (level 9+)
   - No: Continue

2. **Is random access latency critical?**
   - Yes: Use LZ4
   - No: Continue

3. **Is data accessed from S3?**
   - Yes: Use Zstd (level 3-9)
   - No: Use LZ4

4. **Is CPU limited?**
   - Yes: Use LZ4
   - No: Use Zstd

## Changing Compression

To change compression algorithm on existing snapshot:

```bash
# Repack with different compression
hexz data pack \
  --disk original.hxz \
  --output recompressed.hxz \
  --compression lz4
```

Note: This requires reading and rewriting all data.

## Future Algorithms

Potential additions in future versions:
- LZMA (higher compression ratio)
- Brotli (web-optimized)
- Snappy (Google's fast compression)

## See Also

- [ADR-0002: Block-Level Compression](../adr/0002-block-level-compression.md)
- [How-To: Performance Tuning](../how-to/performance-tuning.md)
- [Reference: CLI Commands](cli-reference.md)
- [Explanation: Compression Strategy](../explanation/compression-strategy.md)
