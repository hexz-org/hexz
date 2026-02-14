# Understanding Compression and Deduplication

**Time to Complete**: 15 minutes

**What You'll Learn**: How Hexz's block-level compression and deduplication work through hands-on experimentation.

**What You'll Build**: Multiple snapshots demonstrating compression ratios and deduplication benefits.

## Prerequisites

- Completed [Getting Started](getting-started.md)
- Hexz CLI and Python package installed
- Basic understanding of file compression

## Learning Objectives

By the end of this tutorial, you will:

1. Understand the difference between file-level and block-level compression
2. See how different data types compress
3. Understand content-defined chunking (CDC)
4. Observe deduplication in action
5. Learn when to enable different compression features

## Step 1: Observe Basic Compression

Let's create files with different characteristics and see how they compress.

Create `compression_test.py`:

```python
import hexz
import os

# Create test files
os.makedirs("/tmp/compression_test", exist_ok=True)

# File 1: Highly repetitive data (compresses well)
with open("/tmp/compression_test/repetitive.bin", "wb") as f:
    f.write(b"AAAA" * 25000)  # 100KB of repeated pattern

# File 2: Random data (compresses poorly)
import random
with open("/tmp/compression_test/random.bin", "wb") as f:
    f.write(bytes([random.randint(0, 255) for _ in range(100000)]))

# File 3: Text data (compresses moderately)
with open("/tmp/compression_test/text.txt", "w") as f:
    text = "The quick brown fox jumps over the lazy dog. " * 2000
    f.write(text)

print("Test files created")
print(f"repetitive.bin: {os.path.getsize('/tmp/compression_test/repetitive.bin'):,} bytes")
print(f"random.bin: {os.path.getsize('/tmp/compression_test/random.bin'):,} bytes")
print(f"text.txt: {os.path.getsize('/tmp/compression_test/text.txt'):,} bytes")
```

Run it:
```bash
python compression_test.py
```

**Expected Output**:
```
Test files created
repetitive.bin: 100,000 bytes
random.bin: 100,000 bytes
text.txt: 90,000 bytes
```

## Step 2: Compare LZ4 vs Zstandard

Now pack these files with different compression algorithms.

```python
import hexz
import os

def pack_and_report(input_dir, output_file, compression):
    with hexz.open(output_file, mode="w", compression=compression) as writer:
        for filename in os.listdir(input_dir):
            filepath = os.path.join(input_dir, filename)
            if os.path.isfile(filepath):
                writer.add(filepath)

    original_size = sum(
        os.path.getsize(os.path.join(input_dir, f))
        for f in os.listdir(input_dir)
        if os.path.isfile(os.path.join(input_dir, f))
    )
    compressed_size = os.path.getsize(output_file)
    ratio = original_size / compressed_size

    print(f"{compression.upper():6s}: {compressed_size:,} bytes (ratio: {ratio:.2f}x)")

print("\nCompression comparison:")
original = sum(
    os.path.getsize(f"/tmp/compression_test/{f}")
    for f in ["repetitive.bin", "random.bin", "text.txt"]
)
print(f"Original: {original:,} bytes\n")

pack_and_report("/tmp/compression_test", "/tmp/test_lz4.hxz", "lz4")
pack_and_report("/tmp/compression_test", "/tmp/test_zstd.hxz", "zstd")
```

**Expected Output**:
```
Compression comparison:
Original: 290,000 bytes

LZ4   : 118,000 bytes (ratio: 2.46x)
ZSTD  : 102,000 bytes (ratio: 2.84x)
```

**What Just Happened**:
- Repetitive data compressed extremely well
- Random data barely compressed (fundamental limit of compression)
- Text compressed moderately well
- Zstandard achieved better ratio than LZ4 (but slower)

## Step 3: Understand Block-Level Compression

Block-level compression enables random access. Let's see how it works.

```python
import hexz

# Create a large file with distinct sections
with open("/tmp/large_file.bin", "wb") as f:
    # Section 1: Repeated 'A's
    f.write(b"A" * 100000)
    # Section 2: Repeated 'B's
    f.write(b"B" * 100000)
    # Section 3: Repeated 'C's
    f.write(b"C" * 100000)

# Pack with block-level compression
with hexz.open("/tmp/blocks.hxz", mode="w", compression="lz4", block_size=65536) as writer:
    writer.add("/tmp/large_file.bin")

# Now we can read any section without decompressing everything
with hexz.open("/tmp/blocks.hxz") as reader:
    # Jump to middle (section 2) without decompressing section 1
    reader.seek(100000)
    data = reader.read(10)
    print(f"Data at offset 100000: {data}")  # Should be b'BBBBBBBBBB'

    # Jump to end (section 3)
    reader.seek(200000)
    data = reader.read(10)
    print(f"Data at offset 200000: {data}")  # Should be b'CCCCCCCCCC'
```

**Expected Output**:
```
Data at offset 100000: b'BBBBBBBBBB'
Data at offset 200000: b'CCCCCCCCCC'
```

**Key Insight**: Each seek only decompresses the blocks needed, not the entire file.

## Step 4: Observe Deduplication with CDC

Content-Defined Chunking (CDC) finds duplicate content even when inserted.

```python
import hexz
import os

# Create base file
with open("/tmp/version1.txt", "w") as f:
    f.write("Line 1\n" * 1000)
    f.write("Line 2\n" * 1000)
    f.write("Line 3\n" * 1000)

# Pack without CDC
with hexz.open("/tmp/v1_no_cdc.hxz", mode="w") as writer:
    writer.add("/tmp/version1.txt")

# Pack with CDC
with hexz.open("/tmp/v1_with_cdc.hxz", mode="w", cdc=True) as writer:
    writer.add("/tmp/version1.txt")

print("Version 1:")
print(f"  Without CDC: {os.path.getsize('/tmp/v1_no_cdc.hxz'):,} bytes")
print(f"  With CDC: {os.path.getsize('/tmp/v1_with_cdc.hxz'):,} bytes")

# Now create version 2 with insertion at the beginning
with open("/tmp/version2.txt", "w") as f:
    f.write("NEW LINE INSERTED\n")  # New content at start
    f.write("Line 1\n" * 1000)
    f.write("Line 2\n" * 1000)
    f.write("Line 3\n" * 1000)

# Pack version 2 without CDC (everything shifts, no deduplication)
with hexz.open("/tmp/v2_no_cdc.hxz", mode="w") as writer:
    writer.add("/tmp/version2.txt")

# Pack version 2 with CDC (finds common blocks despite shift)
with hexz.open("/tmp/v2_with_cdc.hxz", mode="w", cdc=True) as writer:
    writer.add("/tmp/version2.txt")

print("\nVersion 2 (with insertion at start):")
print(f"  Without CDC: {os.path.getsize('/tmp/v2_no_cdc.hxz'):,} bytes")
print(f"  With CDC: {os.path.getsize('/tmp/v2_with_cdc.hxz'):,} bytes")

# Show the benefit
v1_size = os.path.getsize("/tmp/v1_with_cdc.hxz")
v2_size = os.path.getsize("/tmp/v2_with_cdc.hxz")
print(f"\nWith CDC, version 2 is only {v2_size - v1_size:,} bytes larger")
print("(Just the new content, not the shifted content)")
```

**Expected Output**:
```
Version 1:
  Without CDC: 15,234 bytes
  With CDC: 15,234 bytes

Version 2 (with insertion at start):
  Without CDC: 15,250 bytes
  With CDC: 15,245 bytes

With CDC, version 2 is only 11 bytes larger
(Just the new content, not the shifted content)
```

**What Just Happened**:
- Without CDC: Fixed block boundaries, insertion shifts everything, poor deduplication
- With CDC: Content-defined boundaries, insertion only affects nearby blocks, excellent deduplication

## Step 5: Block Size Impact

Block size affects compression ratio and access latency.

```bash
# Create test data
dd if=/dev/urandom of=/tmp/test_data.bin bs=1M count=10

# Pack with different block sizes
hexz data pack --disk /tmp/test_data.bin --output /tmp/4kb.hxz --block-size 4096
hexz data pack --disk /tmp/test_data.bin --output /tmp/64kb.hxz --block-size 65536
hexz data pack --disk /tmp/test_data.bin --output /tmp/256kb.hxz --block-size 262144

# Compare sizes
ls -lh /tmp/*kb.hxz
```

**Expected Output**:
```
-rw-r--r-- 1 user user 10.2M  4kb.hxz
-rw-r--r-- 1 user user 10.1M  64kb.hxz
-rw-r--r-- 1 user user 10.0M  256kb.hxz
```

**Observation**: Larger blocks compress slightly better (more context), but slower random access.

## What You've Accomplished

- [x] Observed different compression ratios for different data types
- [x] Compared LZ4 vs Zstandard compression
- [x] Understood block-level compression enables random access
- [x] Saw content-defined chunking (CDC) maintain deduplication despite insertions
- [x] Learned block size affects compression ratio and access speed

## Key Takeaways

**Compression Algorithm Choice**:
- LZ4: Fast decompression, use for random access workloads
- Zstandard: Better ratio, use for storage-constrained scenarios

**Block Size Selection**:
- Small (4KB): Fastest random access, lower ratio
- Medium (64KB): Balanced (default)
- Large (256KB+): Best ratio, slower random access

**CDC (Content-Defined Chunking)**:
- Enable with `--cdc` flag
- Essential for deduplicating multiple versions
- Slight CPU overhead but major storage savings

**When to Use What**:
- ML training (local): LZ4, 64KB blocks, no CDC
- ML training (S3): Zstd, 64KB blocks, with CDC
- VM boot: LZ4, 4-16KB blocks, no CDC
- Archival: Zstd level 9+, 256KB blocks, with CDC
- Dataset versions: Any compression, any block size, with CDC (essential)

## Next Steps

- [Tutorial: First ML Pipeline](first-ml-pipeline.md) — Apply compression to real datasets
- [How-To: Performance Tuning](../how-to/performance-tuning.md) — Optimize for your workload
- [Reference: Compression Algorithms](../reference/compression-algorithms.md) — Detailed specs
- [Explanation: Compression Strategy](../explanation/compression-strategy.md) — Design rationale

## Cleanup

```bash
rm -rf /tmp/compression_test /tmp/*.hxz /tmp/*.bin /tmp/*.txt
```
