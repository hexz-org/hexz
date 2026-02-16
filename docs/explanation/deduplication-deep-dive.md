# Deduplication Deep Dive

Technical deep dive into Hexz's deduplication system.

## Deduplication Overview

Deduplication eliminates redundant data by storing identical content once and referencing it multiple times.

**Key components**:
1. **Chunking**: Divide data into deduplicatable units
2. **Hashing**: Generate fingerprint for each chunk
3. **Lookup**: Check if chunk already exists
4. **Reference**: Point to existing chunk or write new one

## Architecture

### During Packing

```
Input Stream
    ↓
FastCDC Chunker → Variable-sized chunks (16-256KB)
    ↓
Compressor → Compressed chunks
    ↓
BLAKE3 Hasher → 256-bit hash per chunk
    ↓
Dedup Table Lookup → HashMap<Hash, Offset>
    ↓
If hash exists: Add index entry pointing to existing offset
If hash new: Write chunk, add to HashMap, add index entry
```

### Dedup Table Structure

In-memory hash table during packing:

```rust
struct DedupTable {
    map: HashMap<Blake3Hash, ChunkInfo>,
    stats: DedupStats,
}

struct ChunkInfo {
    offset: u64,           // Physical offset in file
    compressed_size: u32,  // Size of compressed chunk
    refcount: u32,         // How many times referenced
}
```

**Memory usage**: Hash (32 bytes) + ChunkInfo (16 bytes) = 48 bytes per unique chunk

For 1 million unique chunks: 48MB memory

### Index Entries

Each block gets an index entry regardless of deduplication:

```rust
struct BlockInfo {
    virtual_offset: u64,      // Logical offset in snapshot
    physical_offset: u64,     // Actual offset in file (may be shared)
    compressed_size: u32,     // Size on disk
    uncompressed_size: u32,   // Size after decompression
    hash: [u8; 32],          // BLAKE3 hash
}
```

**Deduplication indicator**: Multiple BlockInfo entries with same physical_offset

## Hash Function: BLAKE3

BLAKE3 chosen for deduplication because:

**Speed**: Very fast hashing (faster than compression)

**Security**: Cryptographic hash prevents collision attacks

**Parallelism**: Tree-based hashing enables multi-core usage

**Output size**: 256 bits provides strong collision resistance

### Collision Resistance

Probability of collision with N unique chunks:

- N = 1 million: P(collision) ≈ 10^-60 (effectively zero)
- N = 1 billion: P(collision) ≈ 10^-54 (still effectively zero)
- N = 1 trillion: P(collision) ≈ 10^-48 (negligible)

**Practical impact**: Collision probability so low it's ignored. More likely: cosmic ray bit flip, hardware failure, software bug.

### Verification

Paranoid mode (not currently implemented but possible):
1. On hash match, read both chunks
2. Byte-by-byte comparison
3. Only deduplicate if truly identical

Trade-off: Eliminates collision risk but requires reading existing chunks (slower, more I/O).

## Deduplication Scope

### Within Single Snapshot

**Current implementation**: Deduplication within one packing operation

**Process**:
1. Start with empty dedup table
2. Process chunks sequentially
3. Deduplicate against previously seen chunks in same snapshot
4. Discard dedup table after packing completes

**Benefit**: Simple, no persistent state

**Limitation**: Cannot deduplicate across multiple snapshots

### Cross-Snapshot Deduplication (Future)

**Planned enhancement**: Deduplicate across multiple .hxz files

**Approach**:
1. Maintain persistent dedup index (database or index file)
2. When packing, check both in-memory and persistent index
3. Reference blocks from other snapshots
4. Requires careful lifetime management (don't delete referenced snapshots)

**Challenge**: Dependency management (snapshot A references snapshot B, must keep B)

## Deduplication Ratio

Factors affecting deduplication ratio:

### Data Characteristics

**High dedup potential**:
- OS images (shared system files)
- VM snapshots (incremental changes)
- Dataset versions (minor updates)
- Repeated patterns (logs, generated data)

**Low dedup potential**:
- Random data
- Encrypted data
- Highly compressed data (JPEGs, videos)
- Unique content

### Chunking Strategy

**Fixed-size chunking**: Poor dedup after insertions

**CDC (FastCDC)**: Good dedup even after insertions

**Example**: Insert 1KB at start of 100MB file
- Fixed: 0% dedup (all boundaries shift)
- CDC: 99% dedup (only first chunk affected)

### Compression Timing

**Hash before compression**: Lower dedup ratio (uncompressed data has less redundancy)

**Hash after compression** (Hexz's approach): Better dedup ratio (compressed data eliminates local redundancy, finds global duplicates)

## Performance Considerations

### CPU Impact

CDC + hashing adds computational cost:

**Components**:
- Rolling hash for CDC (every byte processed)
- BLAKE3 hash (every chunk hashed)
- HashMap lookup (every chunk queried)

**Mitigation**:
- BLAKE3 is very fast
- HashMap lookups are O(1) average
- Rolling hash is simple gear hash (fast)

**Overall impact**: Acceptable overhead for significant storage savings

### Memory Impact

Dedup table grows with unique chunk count:

**Memory usage** = Unique chunks × 48 bytes

**Examples**:
- 10GB with 64KB avg chunks = 160K chunks = 7.7MB
- 100GB with 64KB avg chunks = 1.6M chunks = 77MB
- 1TB with 64KB avg chunks = 16M chunks = 768MB

**Mitigation**: Large datasets may need substantial RAM for dedup table

### I/O Impact

Deduplication reduces I/O:

**Write**: Only write unique chunks (less disk I/O)

**Read**: Same as non-deduped (index points to correct offset)

**Net benefit**: Less data written, same read performance

## Memory Expectations

The dedup hash table consumes approximately **48 bytes per unique block** (32-byte BLAKE3 hash + 16-byte `ChunkInfo`). The formula:

```
dedup_map_bytes ≈ (input_size / avg_block_size) × 48
```

### Expected memory for common dataset sizes

| Dataset size | Avg block size | Unique blocks | Dedup map memory |
|-------------|---------------|--------------|-----------------|
| 1 GB        | 64 KB         | 16,384       | ~0.75 MB        |
| 10 GB       | 64 KB         | 163,840      | ~7.5 MB         |
| 100 GB      | 64 KB         | 1,638,400    | ~75 MB          |
| 1 TB        | 64 KB         | 16,384,000   | ~750 MB         |

These are worst-case estimates (zero duplication). With typical dedup ratios of 2-4x, actual memory will be proportionally lower since only unique blocks occupy table entries.

> **Note**: Issue #116 tracks capping the dedup hash table memory for very large packs to prevent unbounded growth at TB+ scale. The `pack_memory` benchmark measures RSS to validate any future capping strategy.

## Limitations

### No Cross-File Dedup (Currently)

Packing multiple files separately creates separate snapshots:

```bash
hexz data pack --disk file1.bin --output snapshot1.hxz --cdc
hexz data pack --disk file2.bin --output snapshot2.hxz --cdc
```

Even if file1 and file2 have identical content, no deduplication between snapshot1.hxz and snapshot2.hxz

**Workaround**: Pack files together:

```bash
# Put files in directory
mkdir /tmp/combined
cp file1.bin file2.bin /tmp/combined/

# Pack directory (dedup within snapshot)
hexz data pack --disk /tmp/combined/ --output snapshot.hxz --cdc
```

### Thin Snapshots

Thin snapshots reference parent snapshot:

```bash
hexz data pack --disk v2/ --output v2.hxz --parent v1.hxz --cdc
```

**Limitation**: v2.hxz depends on v1.hxz existing at same path

**Trade-off**: Space savings vs dependency management complexity

### Encryption Breaks Dedup

Encrypted blocks appear random, defeating content-based deduplication.

**Conflict**: Security (encryption) vs efficiency (dedup)

**Workaround**: Encrypt at rest (filesystem or disk level) rather than per-snapshot

## Statistics and Monitoring

Track deduplication effectiveness:

**Metrics**:
- Total chunks processed
- Unique chunks (written to disk)
- Duplicate chunks (referenced existing)
- Dedup ratio = Processed / Unique

**Example output** (hypothetical):
```
Packing complete:
  Chunks processed: 1,600,000
  Unique chunks: 500,000
  Duplicate chunks: 1,100,000
  Deduplication ratio: 3.2x
  Space saved: 68.7%
```

## Best Practices

1. **Enable CDC for version-like data**: Multiple versions of datasets, incremental VM snapshots

2. **Disable CDC for unique data**: Random data, encrypted data, highly compressed data (wastes CPU)

3. **Pack related data together**: Put files that might have duplicates in same snapshot

4. **Consider chunk size**: Smaller = more dedup opportunities but more overhead

5. **Monitor dedup ratio**: If ratio is low (<1.1x), consider disabling CDC

## Future Enhancements

**Planned**:
- Cross-snapshot deduplication with persistent index
- Dedup statistics in info command
- Configurable paranoid mode (byte-compare on hash match)

**Under consideration**:
- Compression-aware chunking (CDC on decompressed data)
- Hierarchical dedup (coarse + fine-grained)
- Distributed dedup (shared index across machines)

## See Also

- [ADR-0003: BLAKE3 and FastCDC Deduplication](../adr/0003-blake3-fastcdc-deduplication.md) - Design rationale
- [Explanation: Content-Defined Chunking](content-defined-chunking.md) - CDC details
- [Tutorial: Understanding Compression](../tutorials/understanding-compression.md) - Hands-on examples
- [Reference: Compression Algorithms](../reference/compression-algorithms.md) - Technical specs
