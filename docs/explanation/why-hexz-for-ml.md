# Why Hexz for Machine Learning

This document explains what Hexz actually solves in ML workflows, where it helps, and where it does not.

---

## What Hexz is

Hexz is a block-compressed archive format with random access and content deduplication. In ML contexts this matters in two places:

1. **Checkpoint versioning** — storing many iterations of the same model without paying full storage cost for each one
2. **Dataset access** — reading arbitrary samples from a compressed archive without extracting it

These are different problems and Hexz solves them to different degrees.

---

## Checkpoint versioning

This is where Hexz has the clearest advantage over existing tools.

A fine-tuned 7B model is ~14GB. Fine-tune it 50 times with different hyperparameters and you have 700GB of nearly-identical files. The standard approaches:

- **Raw file copies**: 700GB. No dedup. Pay for every byte every time.
- **git-lfs**: Tracks which blob corresponds to which version. Does not deduplicate content. Still 700GB.
- **DVC**: Same as git-lfs — a pointer tracker, not a content store. Still 700GB.
- **Hexz thin snapshots**: Store the base model once. Each fine-tune stores only changed blocks. With CDC dedup and ~5% weight change per run, 50 checkpoints costs roughly the space of 3-4 full copies.

**Validated:** The dedup benchmark shows 92.4% deduplication on shifted data (two 50MB versions with 1KB insertion). Fixed-size block dedup breaks to 0% on the same data. See `cargo bench --bench dedup_efficiency`.

**Not yet implemented:** A shared external dedup index across unrelated `.hxz` files. Cross-file dedup currently requires using the thin snapshot parent chain (`hexz build ... --parent v1.hxz`) or packing multiple versions into one archive in a single operation.

---

## Dataset access

Hexz stores training data as a single compressed archive with a two-level index. Any sample can be read in O(log N) index lookups plus one block decompression.

**Validated benchmarks** (Python, real image datasets on i7-14700K):

| | Sequential Read | Random Access | Shuffled Epoch |
|---|---|---|---|
| Local files | 387 MB/s | 6.2 µs | 360 MB/s |
| HDF5 (LZF) | 56 MB/s | 40.8 µs | 55 MB/s |
| **Hexz (LZ4)** | **1,218 MB/s** | **3.4 µs** | **525 MB/s** |

These are on CIFAR-10 (108MB), STL-10 (28MB), CIFAR-100 (18MB). Small datasets that fit entirely in RAM on a fast machine. The numbers show that the Rust I/O path is significantly faster than HDF5's Python path, and faster than the OS file cache path for sequential reads.

**What these numbers do not show:**

- Performance at TB scale from S3. At that scale the bottleneck is network bandwidth (~1-10 Gbps), not decompression. Hexz's sequential throughput is irrelevant when you're waiting on the network.
- Comparison against WebDataset. Those benchmarks are not yet run. Do not assume Hexz beats WebDataset on sequential streaming throughput — WebDataset has a deeply optimized prefetch pipeline tuned for PyTorch's DataLoader.

---

## Why block compression (not file compression)

File-level compression (gzip, zstd over a tar) gives better ratios but zero random access. To read sample #500,000 from a gzip'd tar you decompress from byte 0.

Block-level compression (Hexz) compresses each 64KB block independently. To read sample #500,000 you read and decompress the ~1-2 blocks that contain it. Index lookup: ~174ns warm, ~6.6µs cold.

The tradeoff: ~15-20% worse compression ratio for 1000× better random access latency. For ML training with shuffling this is the right tradeoff.

---

## Why CDC for dataset versioning

Fixed-size block dedup works fine for append-only datasets (new samples added to the end). It breaks when data shifts — when samples are inserted, removed, or modified anywhere in the middle. A 1-byte insertion shifts every subsequent block boundary, making all subsequent blocks appear as new content.

FastCDC computes chunk boundaries based on content, not byte offsets. After an insertion, boundaries re-sync a few blocks later. The validated benchmark: 92.4% of shifted data deduplicated vs 0% with fixed-size. See `cargo bench --bench dedup_efficiency`.

Tradeoff: CDC packing is 2.6× slower (4.9 GB/s → 1.9 GB/s). This is a one-time write cost. Reading CDC-packed archives is the same speed as fixed-size.

---

## Where Hexz does not help

**Sequential streaming at maximum throughput.** WebDataset and MosaicML StreamingDataset have tuned their DataLoader integration for maximum sequential throughput with prefetching. For pure sequential streaming of a dataset you never shuffle, they may match or exceed Hexz.

**Small datasets.** For datasets under 1GB, just use folders. The packing step and index overhead are not worth it.

**Purely tabular data.** Parquet and DuckDB are better for structured data with column queries.

**Single checkpoint, no versioning.** If you save one checkpoint and never compare versions, safetensors is simpler, faster to write (no compression), and universally supported.

---

## Comparison with other formats

| | Random Access | Dedup Across Versions | Single File | No Daemon | S3 Streaming |
|---|---|---|---|---|---|
| Raw files | Yes | No | No | Yes | Slow (per-file requests) |
| tar.gz | No | No | Yes | Yes | No |
| WebDataset | Shard-only | No | No | Yes | Yes |
| HDF5 | Yes (slow) | No | Yes | Yes | Partial |
| safetensors | Yes | No | Yes | Yes | No |
| git-lfs / DVC | No | No (pointer only) | No | No | Yes |
| **Hexz** | **Yes** | **Yes (CDC + thin snapshots)** | **Yes** | **Yes** | **Yes** |

WebDataset comparison is listed but not benchmarked — do not treat the throughput column as validated until those benchmarks are run.

---

## See also

- [Architecture explanation](architecture.md)
- [Content-defined chunking deep dive](content-defined-chunking.md)
- [Benchmarks](../project-docs/BENCHMARKS.md)
- [Competitive comparison](../project-docs/COMPETITIVE_COMPARISON.md)
