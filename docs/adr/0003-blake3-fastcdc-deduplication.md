# 3. Deduplication Using BLAKE3 and FastCDC

Date: Early development phase

## Status

Accepted

## Context

Deduplication is critical for Strata's target workloads:

**ML Datasets**:
- Duplicate images across train/val/test splits
- Repeated augmented samples
- Multiple dataset versions with shared content
- Checkpoints containing identical optimizer state

**VM Images**:
- Base OS blocks shared across all images
- Common libraries and binaries
- Incremental snapshots with minimal changes

Without deduplication, storing multiple versions of large datasets requires significantly more storage.

Key design decisions:

1. **Chunking Algorithm**: How to divide data into deduplicatable units
2. **Hash Function**: How to identify duplicate chunks
3. **Deduplication Scope**: Within single snapshot vs. across snapshots

**Chunking Alternatives**:
- **Fixed-size**: Simple but boundary-shift problem (insert 1 byte = all subsequent blocks different)
- **Content-Defined Chunking (CDC)**: Boundaries based on content, resilient to insertions
  - **Rabin fingerprinting**: Traditional CDC, slower
  - **FastCDC**: Optimized CDC with better performance and chunk size distribution

**Hash Alternatives**:
- **SHA-256**: Standard, widely used
- **BLAKE2**: Faster, secure
- **BLAKE3**: Very fast, modern, parallelizable
- **xxHash**: Non-cryptographic, risky for content addressing

The constraint is detecting duplicates without becoming the bottleneck. Hashing should be faster than compression.

## Decision

We will implement **content-defined chunking using FastCDC with BLAKE3 hashing**.

### Chunking Strategy

- **FastCDC Algorithm**: Normalized chunking with gear-based rolling hash
- **Chunk Size Range**: 16KB min, 64KB average, 256KB max (configurable)
- **Cut Point Mask**: 16-bit mask for ~64KB average (adjustable for storage/dedup trade-off)

When CDC is enabled:
1. Input stream processed with rolling hash
2. Cut points identified when hash & mask == pattern
3. Resulting variable-sized chunks compressed as blocks
4. Fixed-size blocking used when CDC disabled (faster, less deduplication)

### Hash Function

- **BLAKE3**: Cryptographic hash with high throughput
- **256-bit output**: Strong collision resistance
- **Incremental hashing**: Supports tree-based parallelism

### Deduplication Scope

- **Within Snapshot**: Hash map tracks blocks during pack operation
- **Cross-Snapshot**: Future enhancement (requires external index)
- **Storage**: Hash stored in block metadata for verification

### Workflow

```
Input Data
    ↓
FastCDC Chunking (variable sizes: 16-256KB)
    ↓
Per-Chunk Compression (LZ4/Zstd)
    ↓
BLAKE3 Hash (after compression)
    ↓
Hash Table Lookup
    ↓
[Duplicate? → Reuse Offset] or [New? → Write Block + Store Hash]
    ↓
Index Entry (hash, offset, compressed_size)
```

## Consequences

### Positive

- **Resilient Deduplication**: CDC finds duplicates despite insertions and shifts
- **High Throughput**: BLAKE3 hashing is fast and doesn't bottleneck compression
- **Security**: Cryptographic hash prevents collision attacks on content addressing
- **Verification**: Hash serves dual purpose (dedup + integrity checking)
- **Storage Savings**: Significant reduction on datasets with multiple versions
- **Cross-Platform**: BLAKE3 has optimized implementations for x86 and ARM

### Negative

- **CPU Overhead**: CDC + hashing adds CPU cost vs. fixed-size chunking
- **Memory Usage**: Hash table for deduplication requires memory proportional to block count
- **Variable Compression**: CDC chunks have less uniform size, complicates block cache sizing
- **Complexity**: FastCDC algorithm harder to debug than fixed-size chunking
- **No Cross-Snapshot Dedup (Yet)**: Must repack to deduplicate across snapshots (future work)

### Neutral

- **Chunk Size Tuning**: Smaller average = more dedup opportunities but higher index overhead
- **Hash Collision Handling**: 256-bit hash makes collisions astronomically unlikely (verified by reading both blocks on match)
- **Rolling Hash Algorithm**: Gear-based hash (not Rabin) for better performance
- **Dedup Enable Flag**: `--cdc` flag enables CDC; disabled by default for simplicity

## Related Decisions

- See ADR-0002 for block compression strategy
- See explanation/content-defined-chunking.md for algorithm details
- See explanation/deduplication-deep-dive.md for performance analysis
