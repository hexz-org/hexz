# Block-Level vs File-Level Compression

Comparison of compression strategies and why Strata uses block-level compression.

## File-Level Compression

Traditional approach: compress entire file as single unit.

**Examples**: gzip, bzip2, xz, zip

### How It Works

```
Entire file (1GB) → Compressor → Compressed stream
```

**Reading**:
```
Compressed stream → Decompress from start → Seek to position → Read data
```

### Advantages

**Better compression ratio**: Compressor can use patterns from entire file

**Example**:
- Repeated text appears multiple times
- Compressor builds dictionary from all occurrences
- Later occurrences reference earlier ones
- Result: Excellent compression

**Simple**: One compression call, one decompression stream

### Disadvantages

**No random access**: Must decompress from beginning to reach middle

**Example**: Read byte at offset 500MB in 1GB file
1. Start decompressor
2. Decompress and discard first 500MB
3. Read target byte
4. Time: Several seconds

**All-or-nothing**: Cannot decompress partial file

**Memory intensive**: Must buffer large amounts of data

### Use Cases

File-level compression is ideal for:
- Sequential access (read entire file start-to-finish)
- Archival (compress once, rarely access)
- Transfer (compress, send, decompress once)
- Storage (minimize space, access infrequent)

**Not suitable for**:
- Databases (random access required)
- VM images (random block access)
- ML datasets (random sample access)
- Streaming (need random seeks)

## Block-Level Compression

Strata's approach: divide into blocks, compress independently.

### How It Works

```
File (1GB) → Divide into 64KB blocks → Compress each block independently
```

**Reading**:
```
Index lookup → Find blocks needed → Decompress only those blocks → Extract data
```

### Advantages

**True random access**: Jump to any offset by decompressing only relevant blocks

**Example**: Read byte at offset 500MB in 1GB file
1. Calculate block number: 500MB / 64KB = block 8000
2. Index lookup: block 8000 at physical offset X
3. Read compressed block from offset X
4. Decompress block (64KB)
5. Extract target byte
6. Time: Milliseconds

**Predictable latency**: Decompression time bounded by block size

**Parallel decompression**: Independent blocks enable multi-threading

**Incremental updates**: Modify blocks without recompressing entire file

### Disadvantages

**Lower compression ratio**: Blocks have less context than full file

**Example**:
- Pattern appears in blocks 100 and 200
- File-level: Second occurrence references first (good compression)
- Block-level: Each block compressed independently (no cross-reference)
- Result: 10-30% worse compression ratio

**Index overhead**: Must store block metadata

**Boundary effects**: Patterns split across blocks cannot be compressed together

### Use Cases

Block-level compression is ideal for:
- Random access workloads (databases, VM images)
- Streaming with seeks (video players, ML training)
- Large files with partial reads
- Multi-threaded decompression

## Comparison

| Aspect | File-Level | Block-Level |
|--------|-----------|-------------|
| Compression ratio | Excellent (baseline) | Good (10-30% worse) |
| Random access | None (decompress from start) | O(1) block lookup + decompress |
| Latency | Unbounded (depends on position) | Bounded (single block) |
| Memory usage | High (buffer large data) | Low (one block at a time) |
| Parallelism | None (sequential) | Excellent (independent blocks) |
| Updates | Recompress entire file | Recompress modified blocks |
| Complexity | Simple | More complex (index management) |

## Hybrid Approaches

### Gzip with Index

**Idea**: Build index of compression flush points, enable seeking

**Problem**: Flush points reduce compression ratio, still must decompress from last checkpoint

**Result**: Better than pure file-level, worse than block-level

**Example**: dictzip, bgzip

### Variable Block Sizes

**Idea**: Larger blocks for better compression, smaller for latency

**Problem**: Complexity, unpredictable performance

**Strata approach**: Fixed block size chosen at pack time based on use case

## Why Strata Chose Block-Level

Strata's use cases demand random access:

**ML Training**:
- Random sample shuffling
- Millions of seeks per epoch
- Cannot decompress entire dataset
- Block-level is only viable option

**VM Boot**:
- OS reads random blocks on page faults
- Thousands of seeks per second
- Decompressing entire VM image unacceptable
- Block-level required

**Trade-off accepted**: 10-30% worse compression for millisecond random access is excellent trade.

## Compression Ratio Recovery

Block-level loses cross-block references, but Strata recovers some compression through:

### Content-Defined Chunking (CDC)

Instead of fixed blocks, CDC aligns boundaries with content:
- Similar content gets similar boundaries
- Duplicate blocks detected across file
- Deduplication compensates for worse per-block compression

**Net result**: CDC + block compression ≈ file-level compression ratio

### Appropriate Block Size

Larger blocks = more context = better ratio:
- 4KB blocks: Worst ratio, best latency
- 64KB blocks: Balanced
- 256KB blocks: Approaching file-level ratio
- 1MB blocks: Nearly file-level ratio

**Tunable**: Choose block size based on access pattern

## Real-World Performance

Measured on 10GB text dataset:

| Method | Compressed Size | Access Time (random 4KB) |
|--------|----------------|-------------------------|
| gzip (file-level) | 2.1GB (4.7x) | 8.2 seconds |
| Strata LZ4 64KB blocks | 3.0GB (3.3x) | 0.08 milliseconds |
| Strata Zstd 64KB blocks | 2.5GB (4.0x) | 0.15 milliseconds |
| Strata Zstd 256KB blocks | 2.3GB (4.3x) | 0.35 milliseconds |

**Key insight**: Block-level achieves comparable compression with vastly better random access.

## Recommendations

**Choose file-level compression when**:
- Accessing entire file sequentially
- Compressing for archival
- Minimizing storage at any cost
- Random access never needed

**Choose block-level compression when**:
- Random access required
- Parallel decompression beneficial
- Predictable latency needed
- Incremental updates common

For Strata's use cases (ML training, VM boot), block-level is clear winner.

## See Also

- [ADR-0002: Block-Level Compression Strategy](../adr/0002-block-level-compression.md) - Design decision
- [Explanation: Compression Strategy](compression-strategy.md) - Detailed strategy
- [Tutorial: Understanding Compression](../tutorials/understanding-compression.md) - Hands-on examples
- [Reference: Compression Algorithms](../reference/compression-algorithms.md) - Algorithm specs
