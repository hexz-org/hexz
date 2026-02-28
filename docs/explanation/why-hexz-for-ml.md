# Why Hexz for Machine Learning

This document explains what Hexz solves in ML workflows, where it genuinely helps, and where it does not. It avoids inflated claims — unvalidated numbers are marked `[UNTESTED]`.

---

## What Hexz is

Hexz is a block-compressed archive format with content deduplication and random access. For ML workflows it focuses on one thing: storing many versions of large model checkpoint files without paying full storage cost for each one, while keeping individual tensors accessible by name without reading the whole file.

---

## The problem: checkpoint versioning at scale

Fine-tuning a 7B model produces a ~14 GB file. If you fine-tune 50 times:

- **Raw copies:** 700 GB. No dedup.
- **git-lfs:** 700 GB. Tracks which blob corresponds to which commit, does not deduplicate content inside blobs.
- **DVC + S3:** Same as git-lfs — a pointer tracker, not a content store.
- **Hexz (block dedup via parent chain):** Only blocks that differ from the parent are stored. How much this saves depends on how much the weights change per run and which storage algorithm is used.

### CDC block dedup (current, validated)

The existing dedup benchmark (`cargo bench --bench dedup_efficiency`) shows 92.4% deduplication on shifted data — two 50 MB versions where the second has 1 KB inserted at the start. Fixed-size block dedup produces 0% savings on the same data.

This result reflects a synthetic benchmark. Real checkpoint savings depend on:
- How many tensors changed per fine-tune
- How much each changed tensor's raw bytes differ from the parent
- Whether tensor boundaries align with block boundaries (they do, with tensor-level chunking)

### XOR delta compression (Phase 3, in development)

The CDC benchmark measures block-level dedup: a block is either identical to the parent or it isn't. For fine-tuned weights where every tensor changed slightly, most blocks differ and don't dedup.

XOR delta addresses this: for each tensor that exists in both the base and fine-tune, Hexz XORs the raw bytes. Fine-tuning perturbs weights across all parameters without inserting or deleting bytes, so the XOR result has the same size but lower entropy — zstd handles low-entropy data well. The theoretical basis is established (Hachiuma et al., "ZipLLM," 2024); empirical savings on real models with Hexz will be benchmarked as part of Phase 3.

**[UNTESTED: storage cost of fine-tune chains with XOR delta on real models]**

---

## Random access: loading individual tensors

The tensor manifest (stored in the archive header) maps tensor name → (offset, length, dtype, shape). `hexz.checkpoint.load("model.hxz", keys=["lm_head.weight"])` fetches only the byte ranges for those tensors, from local disk or over S3 byte-range requests. The rest of the file is not read.

This matters when:
- Doing inference on a single layer during evaluation
- Loading adapter weights while keeping the base model in VRAM
- Auditing specific tensors without downloading a 14 GB file

**[UNTESTED: latency and throughput of selective tensor loading at scale]**

---

## Dataset access

Hexz stores data as a single compressed archive with a two-level index. Any byte range can be read in O(log N) index lookups plus one block decompression.

**Validated benchmarks** (Python, real image datasets, i7-14700K, NVMe):

| Format | Sequential Read | Random Access | Shuffled Epoch |
|---|---|---|---|
| Local files | 387 MB/s | 6.2 µs | 360 MB/s |
| HDF5 (LZF) | 56 MB/s | 40.8 µs | 55 MB/s |
| **Hexz (LZ4)** | **1,218 MB/s** | **3.4 µs** | **525 MB/s** |

These are on CIFAR-10 (108 MB), a small dataset that fits in RAM. The numbers reflect Rust I/O vs Python I/O overhead, not raw decompression speed. At TB scale from S3, the bottleneck is network bandwidth — these numbers do not predict S3 performance.

WebDataset comparison has not been benchmarked. Do not treat the throughput column as a win over WebDataset until that comparison is run.

---

## Where Hexz does not help

**Single checkpoint, no versioning.** If you save one checkpoint and never compare versions, safetensors is simpler, faster to write (no compression overhead), and universally supported. Hexz adds no value here.

**Need full `torch.load` compatibility.** Hexz cannot load arbitrary pickled Python objects. It's a byte-range store, not a Python serializer.

**Purely sequential streaming of training data.** WebDataset and MosaicML StreamingDataset have optimized DataLoader integration for maximum sequential throughput. Hexz doesn't target this use case.

**Small models or datasets.** For files under 1 GB, the index overhead and packing time are not worth it.

**Team access controls and audit UI.** Weights & Biases Artifacts and MLflow are full platforms with authentication, history UIs, and team features. Hexz is a format.

**Windows.** The mmap and byte-range paths need validation on Windows. Don't deploy it there yet.

---

## Comparison table

| | Random access | Named tensor load | Dedup across versions | XOR delta | Single file | No daemon | S3 streaming |
|---|---|---|---|---|---|---|---|
| Raw files | Yes | No | No | No | No | Yes | Slow |
| git-lfs / DVC | No | No | No | No | No | No | Yes |
| safetensors | Yes | Yes (mmap) | No | No | Yes | Yes | No |
| GGUF | Yes | Yes (mmap) | No | No | Yes | Yes | No |
| **Hexz** | **Yes** | **Yes** | **Yes (parent chain)** | **In dev** | **Yes** | **Yes** | **Yes** |

---

## See also

- [XOR Delta Compression](xor-delta-compression.md)
- [Architecture](architecture.md)
- [Deduplication Deep Dive](deduplication-deep-dive.md)
- [Competitive Comparison](../project-docs/COMPETITIVE_COMPARISON.md)
- [Benchmarks](../project-docs/BENCHMARKS.md)
