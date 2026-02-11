# Compression Algorithms

Comparison of compression algorithms supported by Strata.

## Supported Algorithms

Strata supports two compression algorithms optimized for different use cases.

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
strata data pack --disk data/ --output out.st --compression lz4
```

```python
# Python
strata.open("out.st", mode="w", compression="lz4")
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
strata data pack \
  --disk data/ \
  --output out.st \
  --compression zstd \
  --compression-level 9
```

```python
# Python with compression level
strata.open("out.st", mode="w", compression="zstd", compression_level=9)
```

## Comparison

### Performance

Measured on random data, 64KB blocks:

| Algorithm | Compress | Decompress | Ratio | CPU |
|-----------|----------|------------|-------|-----|
| LZ4 | Fast | Very fast | 2.2x | Low |
| Zstd-1 | Fast | Fast | 2.8x | Low |
| Zstd-3 | Moderate | Fast | 3.4x | Medium |
| Zstd-9 | Slow | Fast | 4.1x | High |
| Zstd-22 | Very slow | Moderate | 5.2x | Very high |

Note: Actual performance varies by data characteristics.

### Storage Savings

Example: 1TB image dataset

| Algorithm | Compressed Size | Bandwidth Saved | Pack Time |
|-----------|----------------|-----------------|-----------|
| None | 1000 GB | 0% | N/A |
| LZ4 | 450 GB | 55% | Fast |
| Zstd-3 | 330 GB | 67% | Moderate |
| Zstd-9 | 280 GB | 72% | Slow |
| Zstd-22 | 220 GB | 78% | Very slow |

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

Compression ratio improves with larger blocks:

| Block Size | LZ4 Ratio | Zstd-9 Ratio | Random Access Latency |
|------------|-----------|--------------|----------------------|
| 4 KB | 1.8x | 2.4x | Lowest |
| 16 KB | 2.0x | 3.0x | Low |
| 64 KB | 2.2x | 3.5x | Moderate (default) |
| 256 KB | 2.4x | 4.2x | Higher |
| 1 MB | 2.6x | 4.8x | Highest |

See [ADR-0002](../adr/0002-block-level-compression.md) for block-level compression rationale.

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
strata data pack \
  --disk original.st \
  --output recompressed.st \
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
