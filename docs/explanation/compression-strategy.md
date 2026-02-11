# Compression Strategy

This document explains Strata's approach to compression and the rationale behind design choices.

## The Compression Problem

Traditional compression formats force a choice:

**Option 1: File-Level Compression (tar.gz)**
- Compress entire file as one unit
- Excellent compression ratio (large context window)
- Cannot seek without full decompression
- Unacceptable for random access workloads

**Option 2: No Compression**
- Instant random access
- Storage costs multiply
- Bandwidth usage increases

Strata needs both: compression for efficiency AND random access for performance.

## Block-Level Compression Approach

Strata divides data into fixed-size blocks and compresses each independently.

### How It Works

```
Input Data (1MB)
    ↓
Divide into blocks (64KB each = 16 blocks)
    ↓
Compress each block independently
    ↓
[Block 0: compressed] [Block 1: compressed] ... [Block 15: compressed]
    ↓
Build index: Block N → Offset + Compressed Size
```

### Random Access Example

To read bytes 200000-200100:

1. Calculate which block contains offset 200000
   - Block size = 64KB = 65536 bytes
   - Block number = 200000 / 65536 = 3
2. Look up block 3 in index
3. Read compressed block 3 from disk
4. Decompress only block 3
5. Extract bytes 200000-200100 from decompressed block

**Key insight**: Only decompress blocks actually needed, not entire file.

## Algorithm Selection

Strata supports two compression algorithms with different trade-offs.

### LZ4

**Characteristics**:
- Very fast decompression
- Moderate compression ratio
- Low CPU usage
- Simple algorithm

**Best for**:
- Random access workloads (ML training with shuffling)
- VM boot (low latency required)
- Local NVMe storage (I/O is fast, CPU is bottleneck)
- Real-time processing

**Example use case**: Training ResNet on local NVMe where decompression speed matters more than storage.

### Zstandard (Zstd)

**Characteristics**:
- Configurable compression levels (1-22)
- Better compression ratio than LZ4
- Still fast decompression
- More CPU intensive

**Best for**:
- S3 streaming (save bandwidth costs)
- Cold storage/archival
- Storage-constrained environments
- Sequential access patterns

**Example use case**: Storing dataset on S3 where bandwidth costs matter.

## Block Size Selection

Block size affects both compression ratio and access latency.

### Small Blocks (4-16KB)

**Advantages**:
- Fastest random access (decompress less data)
- Lower memory usage per block
- Better for tiny random reads

**Disadvantages**:
- Lower compression ratio (less context)
- More index overhead
- More decompression calls

**Use when**: Random access latency is critical (VM boot, database-like access)

### Medium Blocks (64KB, default)

**Advantages**:
- Balanced compression ratio
- Acceptable random access latency
- Reasonable index size

**Disadvantages**:
- Middle-of-the-road on all metrics

**Use when**: General-purpose workload (ML training, most use cases)

### Large Blocks (256KB-1MB)

**Advantages**:
- Best compression ratio (more context)
- Fewer index entries
- Efficient for sequential reads

**Disadvantages**:
- Slower random access (decompress more data per seek)
- Higher memory usage
- Poor for tiny random reads

**Use when**: Sequential access, archival, maximum compression needed

## Compression vs Decompression

Important asymmetry: compression happens once, decompression happens many times.

**Implication**: Optimize for decompression speed, accept slower compression.

**Example**:
- Pack dataset once with Zstd level 9 (slow compression, best ratio)
- Train for 100 epochs, decompressing billions of blocks (fast decompression, good ratio)

Total time saved >> time spent compressing.

## Trade-off Analysis

### Storage vs Speed

```
Storage cost ←→ Access speed

Uncompressed: Expensive storage, fast access
LZ4: Moderate storage, very fast access  ← Sweet spot for local
Zstd-3: Good storage, fast access        ← Sweet spot for S3
Zstd-22: Minimal storage, moderate access ← Cold storage
```

### Block Size vs Latency

```
Compression ratio ←→ Random access latency

4KB blocks: Lower ratio, lowest latency   ← VM boot
64KB blocks: Good ratio, low latency      ← Default
256KB blocks: Better ratio, higher latency ← Sequential
1MB blocks: Best ratio, high latency      ← Archival
```

## Why Not Adaptive Compression?

**Considered but rejected**: Different algorithms per block based on content.

**Reasons**:
1. **Complexity**: Need to track which algorithm for each block
2. **Unpredictability**: Decompression time varies wildly
3. **Marginal benefit**: Content-type detection overhead negates savings

**Decision**: Single algorithm per snapshot, chosen at pack time based on use case.

## Why Block-Level, Not Byte-Level?

**Alternative considered**: Byte-level streaming compression with seekable index.

**Problems**:
1. Must decompress from last checkpoint to seek point
2. Checkpoints require full flush, reducing compression ratio
3. Complex state management
4. Poor parallelization (decompression is sequential)

**Block-level wins**: True random access, parallel decompression, simpler implementation.

## Integration with Deduplication

Block compression integrates with CDC (content-defined chunking) for deduplication:

1. CDC divides input into variable-sized chunks
2. Each chunk compressed independently
3. Hash computed on compressed chunk
4. Duplicate chunks reference same compressed data

See [Content-Defined Chunking](content-defined-chunking.md) for details.

## Real-World Performance

Measured on typical ML dataset (images):

**Scenario**: 100GB of JPEG images (already compressed)
- Uncompressed: 100GB
- LZ4: 98GB (JPEGs don't compress further, but index adds value)
- Zstd-9: 96GB (minimal additional compression)

**Scenario**: 100GB of uncompressed images (PNG, BMP)
- Uncompressed: 100GB
- LZ4: 45GB
- Zstd-9: 32GB

**Key insight**: Compression benefit depends heavily on input data type.

## Recommendations by Use Case

| Use Case | Algorithm | Block Size | Rationale |
|----------|-----------|------------|-----------|
| ML training (local NVMe) | LZ4 | 64KB | Fast decompression, random access |
| ML training (S3) | Zstd-3 or Zstd-9 | 64KB | Save bandwidth, still fast decompression |
| VM boot | LZ4 | 4-16KB | Minimize latency |
| Archival/cold storage | Zstd-15 to Zstd-22 | 256KB-1MB | Maximize compression |
| Frequent updates | LZ4 | 64KB | Fast repack |
| Large sequential files | Zstd-9 | 256KB | Better ratio, sequential reads |

## Future Directions

**Potential improvements**:
- Compression level per block (complex but powerful)
- Additional algorithms (Brotli, LZMA)
- Adaptive block sizing based on content
- Hardware acceleration (QAT, GPU decompression)

**Not planned**:
- Transparent compression upgrade (breaks random access guarantees)
- Mixed algorithms in single snapshot (complexity not worth benefit)

## See Also

- [ADR-0002: Block-Level Compression](../adr/0002-block-level-compression.md) - Decision rationale
- [Tutorial: Understanding Compression](../tutorials/understanding-compression.md) - Hands-on examples
- [Reference: Compression Algorithms](../reference/compression-algorithms.md) - Technical specs
- [How-To: Performance Tuning](../how-to/performance-tuning.md) - Optimization guide
