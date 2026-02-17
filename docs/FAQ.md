# Hexz FAQ

---

## What is Hexz?

Hexz is a **seekable, block-compressed, content-deduplicated binary archive format** written in Rust, with Python bindings and a CLI.

The core primitive: store large binary data compressed, access any byte range without decompressing the whole archive, and deduplicate identical content across versions.

Primary use cases:
1. **Checkpoint versioning** — store many iterations of a model without paying full storage cost for each one
2. **Dataset storage** — random-access reads from a compressed single-file archive over local disk, HTTP, or S3

---

## How is it different from tar.gz or zip?

Traditional archives require sequential decompression. To read byte 1,000,000 from a gzip file you must decompress from byte 0.

Hexz uses **block-level compression**: data is split into 64KB blocks, each compressed independently. To read at offset 1,000,000 you look up which block(s) cover that range (O(log N) index lookup), decompress only those blocks, and return the slice. Cold access latency: ~6.6 µs. Warm (cached): ~174 ns.

Tradeoff: ~15-20% worse compression ratio than file-level compression. Worth it whenever you shuffle, seek, or do random access.

---

## How is it different from HDF5 or Zarr?

HDF5 and Zarr are designed for **structured array data** — they require you to define chunk shapes, dtypes, and a schema. They're good for scientific array datasets.

Hexz stores **arbitrary bytes** with no schema. It's better for blob data (model weights, image files, disk images) where you don't want to define a schema and just need fast access to named byte ranges.

In benchmarks, HDF5 with variable-length arrays (the mode you'd use for variable-size images) is 20-25× slower than local files due to Python overhead. Hexz is 3-4× faster than local files on the same data.

---

## How is it different from WebDataset?

WebDataset shards data into thousands of tar files and streams them sequentially. It has a well-optimized PyTorch DataLoader integration with prefetching tuned for sequential workloads.

Hexz is a single file with a two-level index enabling true per-sample random access. Differences:

- Hexz supports true global shuffling. WebDataset shuffles within shards only.
- Hexz has content deduplication across versions. WebDataset has none.
- WebDataset's sequential streaming throughput is not yet benchmarked against Hexz. Do not assume Hexz wins there — WebDataset has heavily optimized that path.

If you only do sequential streaming and never need deduplication or version management, WebDataset is simpler and has broader ecosystem support.

---

## How fast is it?

**Engine microbenchmarks (single-threaded, Rust, i7-14700K):**

| Operation | Throughput |
|---|---|
| LZ4 decompress | 32.1 GB/s |
| LZ4 compress | 23.6 GB/s |
| Sequential read (100MB file) | 9.0 GB/s |
| Pack LZ4, no CDC | 4.9 GB/s |
| Pack LZ4 + CDC | 1.9 GB/s |
| Random access, cold cache | 6.6 µs |
| Random access, warm cache | 174 ns |

These are microbenchmarks on data in RAM. They measure the engine, not end-to-end training throughput.

**Python data loading (real image datasets):**

Hexz is 3-4× faster than local files and significantly faster than HDF5 in Python-level benchmarks on CIFAR-10, STL-10, and CIFAR-100. See [BENCHMARKS.md](project-docs/BENCHMARKS.md) for full numbers.

These benchmarks are on small datasets (~100MB) that fit in RAM. For TB-scale datasets from S3, the network is the bottleneck. The engine numbers above become irrelevant.

---

## Is it fast enough for GPU training?

For datasets that fit in local RAM or NVMe, yes — the I/O path is not the bottleneck.

For datasets streamed from S3: it depends entirely on your network bandwidth, not Hexz's decompression speed. A 10 Gbps link to S3 gives you ~1.25 GB/s regardless of what format you use. Hexz's S3 backend uses HTTP byte-range requests to fetch only the blocks you need, which helps for random access but doesn't increase total bandwidth.

---

## Why is CDC slower than fixed-size packing?

FastCDC computes a rolling hash over every byte to find content-defined chunk boundaries. This costs ~2.7 GB/s throughput regardless of other settings. Fixed-size chunking just slices at fixed offsets — no hashing needed — so it runs at ~26 GB/s.

Result: CDC packing is 2.6× slower (4.9 GB/s → 1.9 GB/s).

CDC only affects **packing time** (write). Reading a CDC-packed archive is the same speed as reading a fixed-size-packed archive.

---

## When should I use CDC vs fixed-size blocks?

Use **fixed-size** (default) when:
- You only append new data to datasets (never insert or modify)
- Pack speed matters more than dedup quality

Use **CDC** when:
- Data is modified or samples are inserted between versions
- You're versioning model checkpoints (weights change in-place across runs)
- You want the 92.4% dedup on shifted data rather than 0% with fixed-size

The validated benchmark: on a 50MB base + 50MB version-with-1KB-insertion:
- Fixed-size: 0% dedup (boundary shift breaks every block)
- CDC: 92.4% dedup (boundaries re-sync after the insertion)

---

## Does CDC affect read performance?

No. Once packed, CDC and fixed-size archives read at the same speed.

---

## Can Hexz deduplicate across multiple separate files?

**Partially.** Two mechanisms exist:

1. **Thin snapshots (parent-child chain)**: Set `parent_path` in the header when packing. The child only stores blocks not already in the parent. Works for a linear chain (v1 → v2 → v3). The parent must be accessible at read time.

2. **Single-operation dedup**: When packing multiple inputs in one operation, a shared in-memory hash table deduplicates across them.

**Not yet implemented:** A shared external dedup index that lets you deduplicate across unrelated `.hxz` files without a parent-child relationship. This is planned.

---

## How does encryption work?

AES-256-GCM, per block, with AES-NI hardware acceleration (~2.1 GB/s encrypt/decrypt).

Each block gets a unique nonce. This means encrypted blocks cannot be deduplicated — two identical plaintext blocks produce different ciphertext. Encryption and dedup are mutually exclusive.

Ed25519 signing is separate from encryption and can be used on unencrypted archives.

---

## What Python API is actually implemented?

```python
import hexz

# Open for reading
with hexz.open("data.hxz") as reader:
    data = reader.read()           # read all
    chunk = reader.read(4096)      # read N bytes
    reader.seek(1024)              # seek
    block = reader[100:200]        # slice

# Open for writing
with hexz.open("output.hxz", mode="w", compression="lz4") as writer:
    writer.add_file("disk.img")
    writer.add_bytes(b"extra data")

# Build from source
hexz.build("source.img", "output.hxz", profile="ml")

# Inspect
hexz.inspect("data.hxz")

# Verify
hexz.verify("data.hxz")

# Array access
hexz.read_array("data.hxz", offset=0, shape=(1000, 768), dtype="float32")

# PyTorch Dataset
from hexz import Dataset
dataset = Dataset("train.hxz", item_size=3073, shuffle=True)

# Crypto
hexz.keygen("private.key", "public.key")
hexz.sign("snapshot.hxz", "private.key")
hexz.verify("snapshot.hxz", "public.key")
```

**Not yet implemented** (planned, not shipped):
- `reader.read_sample(idx)` — per-sample indexed access
- `reader.get_metrics()` / `enable_metrics=True` — runtime statistics
- `hexz.checkpoint.save()` / `hexz.checkpoint.load()` — tensor-manifest checkpoint API
- `hexz merge` — merge two archives

---

## What does Windows support look like?

The Rust core and Python bindings build on Windows. The FUSE mount and some mmap paths have not been validated. Do not treat Windows as a supported platform for production use.

---

## What's the file format?

```
[Header, 4096 bytes]
[Data blocks, variable]
[Page indices, variable]
[Master index, variable]
```

Header contains compression type, encryption params, index offset, optional parent path, optional metadata blob offset. All integers little-endian.

Data blocks are independently compressed (and optionally encrypted) chunks. The two-level index maps virtual byte offsets to physical block locations. Block lookup is O(log P + log B) where P = number of page index pages, B = blocks per page.

Full spec: [file-format-spec.md](reference/file-format-spec.md)

---

## How do I report a bug or ask a question?

- [GitHub Issues](https://github.com/hexz-org/hexz/issues)
- [GitHub Discussions](https://github.com/hexz-org/hexz/discussions)
