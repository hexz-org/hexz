# Content-Defined Chunking Explained

This document explains how content-defined chunking (CDC) enables deduplication in Hexz.

## The Problem: Fixed-Size Chunking

Traditional deduplication uses fixed-size blocks. This has a fundamental flaw called the **boundary shift problem**.

### Example: Fixed-Size Chunking Fails

Consider a file divided into 64KB fixed blocks:

**Version 1**:
```
[Block 0: bytes 0-65535]
[Block 1: bytes 65536-131071]
[Block 2: bytes 131072-196607]
```

Now insert 1 byte at the beginning:

**Version 2**:
```
[Block 0: NEW BYTE + bytes 0-65534]
[Block 1: bytes 65535-131070]
[Block 2: bytes 131071-196606]
```

**Problem**: All block boundaries shifted by 1 byte. Despite 99.999% identical content, ZERO blocks match. Deduplication completely fails.

## Solution: Content-Defined Chunking

CDC uses the **content itself** to determine block boundaries, not fixed positions.

### How CDC Works

Instead of "cut every 64KB", CDC says "cut when the data looks a certain way".

**Algorithm**:
1. Compute a rolling hash over a small window (e.g., 64 bytes)
2. When hash matches a pattern (e.g., hash & 0xFFFF == 0), mark a cut point
3. Result: blocks of variable size, but boundaries based on content

**Key insight**: Inserting a byte only affects blocks near the insertion. Distant blocks have the same boundaries because their content hasn't changed.

### Example: CDC Succeeds

**Version 1** with CDC:
```
[Block A: "...contentA" (hash matches pattern)]
[Block B: "...contentB" (hash matches pattern)]
[Block C: "...contentC" (hash matches pattern)]
```

**Version 2** (insert at beginning):
```
[Block A': NEW BYTE + partial contentA (hash matches pattern)]
[Block B: "...contentB" (hash matches pattern)] ← SAME as Version 1!
[Block C: "...contentC" (hash matches pattern)] ← SAME as Version 1!
```

**Result**: Blocks B and C unchanged, detected as duplicates, deduplicated successfully.

## FastCDC Algorithm

Hexz uses FastCDC, an optimized CDC variant.

### Why FastCDC?

Traditional CDC (Rabin fingerprinting) is slow. FastCDC improves speed with:

1. **Gear hash**: Simpler than Rabin, faster computation
2. **Normalized chunking**: Better size distribution (avoids tiny or huge blocks)
3. **Mask-based matching**: Fast bitwise operations

### Parameters

**Chunk size bounds**:
- Minimum: 16KB (prevent tiny blocks)
- Average: 64KB (controlled by mask)
- Maximum: 256KB (prevent huge blocks)

**Mask**: 16-bit mask (0xFFFF) gives average 64KB chunks

**Example**:
- Mask 0xFFFF (16 bits set): Average 2^16 = 64KB
- Mask 0xFFF (12 bits set): Average 2^12 = 4KB
- Mask 0xFFFFF (20 bits set): Average 2^20 = 1MB

### Normalized Chunking

FastCDC enforces min/max bounds to prevent pathological cases:

```python
while True:
    if size >= max_size:  # Hit maximum
        cut_here()
        break
    if size >= min_size and hash_matches_pattern():  # Normal cut
        cut_here()
        break
    advance_window()
```

This ensures reasonable chunk sizes even in worst-case data.

## Deduplication Workflow

How CDC integrates with compression and deduplication:

```
Input Data
    ↓
FastCDC: Find cut points based on content
    ↓
Variable-sized chunks (16-256KB)
    ↓
Compress each chunk (LZ4/Zstd)
    ↓
Hash compressed chunk (BLAKE3)
    ↓
Check if hash exists in dedup table
    ↓
If exists: Reference existing chunk (deduplication!)
If new: Write chunk, add hash to table
```

**Key point**: Hashing happens AFTER compression, on compressed data.

## When CDC Helps

CDC provides significant benefit when:

1. **Multiple versions with minor changes**
   - Dataset v1 vs v2 with 5% changes
   - VM snapshot before and after updates
   - Code repository with commits

2. **Shifted content**
   - Log files with timestamps prepended
   - Datasets with headers added
   - Files with insertions/deletions

3. **Partially duplicated files**
   - Similar images with different metadata
   - Documents with common boilerplate
   - Executables with shared libraries

## When CDC Doesn't Help

CDC provides little benefit when:

1. **Completely unique data**
   - Random data
   - Already-deduplicated data
   - Encrypted data

2. **First version**
   - Nothing to deduplicate against
   - CDC still works, just no space savings yet

3. **Heavily compressed data**
   - JPEGs, videos, compressed archives
   - Already minimal redundancy

## Performance Impact

CDC adds computational cost:

**CPU overhead**:
- Rolling hash computation for every byte
- Boundary detection logic
- Hash table lookups

**Memory overhead**:
- Dedup hash table grows with unique block count
- Typically manageable (hash pointers, not full data)

**Trade-off**: Extra CPU and memory for significant storage savings.

## CDC vs Fixed Chunking Comparison

| Scenario | Fixed Chunking | CDC | Winner |
|----------|---------------|-----|--------|
| Insert 1 byte at start | 0% dedup | 95%+ dedup | CDC |
| Append to end | 100% dedup | 100% dedup | Tie |
| Modify middle | 1 block changes | 1-3 blocks change | CDC |
| Completely new data | N/A | N/A | Tie |
| Random data | N/A | N/A | Tie |

## Enabling CDC in Hexz

```bash
# Pack with CDC enabled
hexz data pack \
  --disk data/ \
  --output dataset.hxz \
  --cdc

# Without CDC (fixed-size blocking)
hexz data pack \
  --disk data/ \
  --output dataset.hxz
```

**Python**:
```python
# With CDC
with hexz.open("dataset.hxz", mode="w", cdc=True) as writer:
    writer.add("data/")

# Without CDC
with hexz.open("dataset.hxz", mode="w") as writer:
    writer.add("data/")
```

## Tuning CDC Parameters

Advanced: Adjust CDC behavior for specific needs.

**Smaller chunks** (more dedup opportunities, more index overhead):
```bash
hexz data pack --disk data/ --output dataset.hxz --cdc --min-chunk-size 8192 --avg-chunk-size 32768
```

**Larger chunks** (less overhead, less dedup granularity):
```bash
hexz data pack --disk data/ --output dataset.hxz --cdc --min-chunk-size 32768 --avg-chunk-size 131072
```

## Real-World Example

**Scenario**: 10 versions of a 100GB dataset, each version has 5% changes.

**Without CDC**:
- Total size: 10 × 100GB = 1000GB
- Each version stored completely

**With CDC**:
- Version 1: 100GB (initial)
- Versions 2-10: ~5GB each (only changes)
- Total size: 100GB + (9 × 5GB) = 145GB
- Savings: 85.5%

## Limitations

CDC cannot deduplicate:

1. **Encrypted data**: Encryption destroys content patterns
2. **Compressed data**: Random-looking after compression
3. **Completely rewritten data**: No common content to find

For these cases, CDC adds overhead without benefit. Disable CDC.

## Implementation Notes

Hexz's CDC implementation:

- Uses gear-based rolling hash (not Rabin)
- Enforces min/max bounds (normalized chunking)
- Computes hash after compression (dedup compressed blocks)
- Uses BLAKE3 for content hashing (fast, secure)
- Stores hashes in-memory during packing (for single-snapshot dedup)

Cross-snapshot deduplication (dedup across multiple .st files) is planned for future versions.

## See Also

- [ADR-0003: BLAKE3 and FastCDC Deduplication](../adr/0003-blake3-fastcdc-deduplication.md) - Decision rationale
- [Tutorial: Understanding Compression](../tutorials/understanding-compression.md) - Hands-on CDC examples
- [Explanation: Deduplication Deep Dive](deduplication-deep-dive.md) - Technical deep dive
- [How-To: Performance Tuning](../how-to/performance-tuning.md) - When to enable CDC
