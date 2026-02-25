# Hexz FAQ

---

## What is Hexz?

Hexz is a **seekable, block-compressed, content-deduplicated binary archive format** written in Rust, with Python bindings and a CLI.

The core primitive: store large binary data (like ML model weights) compressed, access any byte range (like a single layer) without decompressing the whole archive, and deduplicate identical content across versions using Content-Defined Chunking (CDC).

Primary use cases: ML checkpoint versioning (storing many iterations of a model without full storage cost for each one) and dataset access (reading arbitrary samples from a compressed archive without extracting it).

---

## How is it different from tar.gz or zip?

Traditional archives require sequential decompression. To read byte 1,000,000 from a gzip file you must decompress from byte 0.

Hexz uses **block-level compression**: data is split into 64KB blocks, each compressed independently. To read at offset 1,000,000 you look up which block(s) cover that range (O(log N) index lookup), decompress only those blocks, and return the slice. Cold access latency on an NVMe machine: ~6.6 µs. Warm (cached): ~174 ns.

Tradeoff: ~15-20% worse compression ratio than file-level compression. Worth it whenever you need random access to parts of a huge file (e.g. loading layers over S3).

---

## How is it different from Safetensors?

Safetensors is a great industry standard for storing tensors without the security risks of Python's `pickle`. However, Safetensors does not support:
1. **Deduplication**: Every version of a model is a full copy.
2. **Compression**: Safetensors is typically uncompressed to allow zero-copy `mmap`.

Hexz combines the safety of raw byte storage with **CDC-based deduplication** and **transparent compression**. It's designed for the *storage* and *versioning* of those weights, whereas Safetensors is designed for the *runtime loading* of a single version.

---

## How is it different from HDF5 or Zarr?

HDF5 and Zarr are designed for **structured array data** — they require you to define chunk shapes, dtypes, and a schema. They're good for scientific array datasets.

Hexz stores **arbitrary bytes** with no schema. It's better for blob data (model weights, image files) where you just need fast, deduplicated access to named byte ranges without defining a rigid coordinate system.

---

## How fast is it?

**Engine microbenchmarks (single-threaded, Rust, i7-14700K):**

| Operation | Throughput |
|---|---|
| LZ4 decompress | 32.1 GB/s |
| LZ4 compress | 23.6 GB/s |
| Sequential read | 9.0 GB/s |
| Pack LZ4, no CDC | 4.9 GB/s |
| Pack LZ4 + CDC | 1.9 GB/s |
| Random access, cold cache | 6.6 µs |
| Random access, warm cache | 174 ns |

These are microbenchmarks on data in RAM. For checkpoints stored on S3, the bottleneck is usually your network bandwidth (~1-10 Gbps), but Hexz's random access means you download **far less data** to load a specific subset of weights.

---

## Why is CDC slower than fixed-size packing?

FastCDC computes a rolling hash over every byte to find content-defined chunk boundaries. This costs ~2.7 GB/s throughput regardless of other settings. Fixed-size chunking just slices at fixed offsets — no hashing needed — so it runs at ~26 GB/s.

Result: CDC packing is 2.6× slower (4.9 GB/s → 1.9 GB/s).

CDC only affects **packing time** (write). Reading a CDC-packed archive is the same speed as reading a fixed-size-packed archive. For model versioning with data that shifts across versions, CDC dedup typically outweighs the one-time write overhead — but savings depend heavily on how much content actually repeats.

---

## When should I use CDC vs fixed-size blocks?

Use **fixed-size** when:
- You have a single version of a file and will never store a derivative of it.
- Pack speed matters more than storage cost.

Use **CDC** when:
- You are storing multiple versions of the same data (e.g. fine-tuned checkpoints).
- Content shifts across versions (insertions, removals, in-place edits).
- The validated dedup benchmark shows 92.4% savings on shifted data vs 0% with fixed-size.

---

## Can Hexz deduplicate across multiple separate files?

**Yes.** Two mechanisms exist:

1. **Cross-file deduplication (New)**: When creating a new snapshot, you can provide a `parent` snapshot. Hexz will automatically scan the parent's block hashes and store lightweight references for any identical chunks it finds in the new file.
2. **Thin snapshots (parent-child chain)**: A child file explicitly references its parent. The child only stores blocks not already in the parent.

This enables storing a fine-tuned model checkpoint in significantly less space than a full copy, depending on how many layers changed.

---

## How does encryption work?

AES-256-GCM, per block, with AES-NI hardware acceleration (~2.1 GB/s encrypt/decrypt).

Each block gets a unique nonce. Note that encrypted blocks cannot be deduplicated — two identical plaintext blocks produce different ciphertext. Deduplication and encryption are currently mutually exclusive in the builder.

---

## What Python API is actually implemented?

```python
import hexz
import torch

# Open for reading (S3/HTTP supported)
with hexz.open("model.hxz") as reader:
    # Random access read
    weights_raw = reader.read(length, offset=layer_offset)
    weights = torch.frombuffer(weights_raw, dtype=torch.float32)
    
    # Metadata access
    meta = reader.metadata
    print(f"Primary size: {meta.primary_size}")

# Open for writing with cross-file deduplication
# This is the "Pivot" use case: FT against a base model
with hexz.Writer("finetuned.hxz", parent="base.hxz", cdc=True) as writer:
    writer.add_bytes(ft_weights)
    writer.add_metadata({"epoch": 10, "base": "llama-7b"})

# Build from directory
hexz.build("/path/to/weights", "output.hxz")

# Inspection
info = hexz.inspect("data.hxz")
print(f"Blocks: {info.num_blocks}, Savings: {info.ratio:.2f}x")
```

---

## What about Windows?

The Rust core and Python bindings build on Windows. However, the FUSE mount and high-performance mmap paths have not been fully validated on Windows yet. We recommend Linux for production ML workloads.

---

## What's the file format?

```
[Header, 4096 bytes]
[Data blocks, variable]
[Page indices, variable]
[Master index, variable]
```

Header contains compression type, encryption params, index offset, optional parent path, and metadata blob offset.

Data blocks are independently compressed chunks. The two-level index maps virtual byte offsets to physical block locations. Block lookup is O(log P) where P is the number of index pages.

---

## How do I report a bug or ask a question?

- [GitHub Issues](https://github.com/hexz-org/hexz/issues)
- [GitHub Discussions](https://github.com/hexz-org/hexz/discussions)