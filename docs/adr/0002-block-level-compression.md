# 2. Block-Level Compression Strategy

Date: Early development phase

## Status

Accepted

## Context

Traditional compression approaches for large datasets face a fundamental trade-off:

**File-Level Compression** (tar.gz, zip):
- Excellent compression ratios (entire file context)
- Requires full decompression to access any byte
- Unacceptable for random access workloads (ML training, VM boot)

**Uncompressed Storage**:
- Instant random access
- 3-10× larger storage costs
- Higher bandwidth requirements for remote datasets

**Streaming Formats** (gzip with index):
- Seeks are expensive (must decompress from last checkpoint)
- Poor performance for truly random access patterns

Hexz's use cases demand both:
1. **ML Training**: Random sample access with shuffling (millions of seeks per epoch)
2. **VM Boot**: Random block reads as OS pages fault (thousands of seeks per second)

The constraint is achieving good compression while maintaining low random access latency.

## Decision

We will use **fixed-size block compression** where:

1. Input data is divided into uniform blocks (default 64KB)
2. Each block is compressed independently (LZ4 or Zstandard)
3. A block index maps logical offsets to physical compressed block locations
4. Reads decompress only the minimum set of blocks needed

Block sizes are configurable:
- **4KB**: Maximum random access speed, lower compression ratio
- **64KB**: Balanced (default for ML workloads)
- **256KB**: Higher compression ratio, acceptable latency for sequential reads
- **1MB+**: Near file-level compression for cold storage

Compression algorithms supported:
- **LZ4**: Fast decompression, moderate compression ratio, ideal for hot data
- **Zstandard**: Good compression ratio, configurable levels, ideal for cold storage

## Consequences

### Positive

- **True Random Access**: Any byte accessible in O(log N) index lookup + single block decompression
- **Predictable Latency**: Block decompression time is bounded and consistent
- **Memory Efficiency**: Only decompressed blocks cached, not entire file
- **Parallel Decompression**: Independent blocks enable multi-threaded decompression without coordination
- **Incremental Updates**: Modified blocks can be appended without recompressing unchanged data
- **Tunable Trade-offs**: Users can select block size based on their access patterns

### Negative

- **Lower Compression Ratio**: Worse than file-level compression (blocks have less context)
- **Storage Overhead**: Index metadata adds small overhead
- **Boundary Effects**: Data split mid-pattern cannot leverage cross-block redundancy
- **Small File Inefficiency**: Files smaller than block size waste space (mitigated by file-level packing)

### Neutral

- **Index Structure**: Master index maps virtual address space to page indices, which contain block metadata
- **Zero Block Optimization**: All-zero blocks stored as metadata-only (no physical storage)
- **Compression Level**: Zstd levels configurable at pack time (not runtime changeable)
- **Cache Behavior**: Decompressed blocks cached in LRU (default 256MB pool)

## Related Decisions

- See ADR-0003 for deduplication strategy (cross-block redundancy)
- See explanation/caching-strategy.md for cache tuning
- See reference/file-format-spec.md for index structure details
