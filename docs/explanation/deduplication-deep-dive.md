# Deduplication Deep Dive

Technical deep dive into Hexz's deduplication systems: block-level BLAKE3 dedup, content-defined chunking, tensor-boundary chunking, and XOR delta compression (Phase 3).

---

## Overview

Hexz has three layers of deduplication, applied depending on the storage mode:

1. **Block dedup (all modes)** — BLAKE3 hash of each compressed block. If the hash matches any block already written (or present in the parent), the block is referenced, not re-written. Cost: zero bytes on disk for identical blocks.

2. **Tensor-boundary chunking (safetensors/GGUF)** — instead of rolling-hash CDC, chunk at the tensor boundaries declared in the file header. Gives stable, predictable block identities for tensor-level dedup.

3. **XOR delta compression (Phase 3, in development)** — for tensors that exist in both base and fine-tune with matching shapes, store the XOR of their bytes instead of the raw bytes. Low-entropy XOR output compresses well with zstd.

---

## Block dedup

### Data structure

```rust
struct DedupTable {
    map: HashMap<Blake3Hash, ChunkInfo>,
    stats: DedupStats,
}

struct ChunkInfo {
    offset: u64,           // Physical offset in file (or parent file)
    compressed_size: u32,
    refcount: u32,
}
```

**Memory usage:** 48 bytes per unique block (32-byte hash + 16-byte `ChunkInfo`).

For a 14 GB model file with 65 KB blocks: ~215,000 blocks → ~10 MB dedup table.

### Dedup scope

**Within a single pack operation:** blocks are deduped against each other as they are written.

**Against a parent archive:** when `--base parent.hxz` is specified, the parent's block index is loaded into the dedup table at pack start. Blocks in the child that hash-match a block in the parent are recorded as `DedupRef` entries pointing to the parent file. The child `.hxz` file stores no bytes for those blocks.

**Cross-archive without parent chain:** not currently supported. Two independently-packed archives of the same model do not share blocks. Use the parent chain (`--base`) for checkpoint versioning.

### BLAKE3

BLAKE3 is used because it is fast (faster than compression in most cases), parallel (tree-based hashing), and produces 256-bit output with strong collision resistance.

Collision probability at 1 million unique blocks: P ≈ 10⁻⁶⁰. Negligible.

### Validated benchmark: CDC dedup on shifted data

```bash
cargo bench --bench dedup_efficiency
```

| Method | Base only | Base + shifted version | Dedup of shifted version |
|---|---|---|---|
| Fixed-size blocks | 50.2 MB | 100.4 MB | **0%** |
| CDC blocks | 50.2 MB | 54.0 MB | **92.4%** |

**Benchmark conditions:** two 50 MB synthetic files; second file has 1 KB inserted at the start. Both packed into one archive with shared dedup map.

Fixed-size block dedup fails entirely because a 1-byte insertion shifts every subsequent block boundary. CDC computes boundaries from content, so they re-sync after the insertion.

**Note:** this benchmark measures CDC dedup on shifted generic data, not tensor-boundary chunking on model files. For model files, tensor boundaries don't shift at all between base and fine-tune (same architecture), making the dedup even more effective for identical tensors.

---

## Content-defined chunking (CDC)

CDC is used when packing generic files with `hexz pack --cdc`. It is **not** used when packing safetensors or GGUF files — tensor-boundary chunking is used instead.

FastCDC with a Gear rolling hash scans the input and cuts at positions where the low N bits of the rolling hash equal a target value. This produces variable-size chunks whose boundaries depend on the data content, not the byte offset.

**Chunk sizes (hexz pack default):**
- Minimum: 16 KiB
- Average: 64 KiB
- Maximum: 256 KiB

**DCAM (`--dcam`):** runs an analysis pass over the file first to fit CDC parameters to the data's actual entropy structure. Useful for heterogeneous data (binary + text + compressed sections). Slow on large files; opt-in only.

**CDC packing speed tradeoff:** `cargo bench --bench throughput`
- Fixed chunking: ~4.9 GB/s
- CDC chunking: ~1.9 GB/s (2.6× slower — one-time write cost, read speed is identical)

---

## Tensor-boundary chunking

For safetensors and GGUF files, Hexz does not run CDC. The file header provides a complete manifest of tensor names, shapes, dtypes, and byte offsets. Hexz uses this directly:

1. Sort tensors by `data_start` (natural file order).
2. For each tensor, write its bytes in `block_size` chunks.
3. Pad to `block_size` boundary with zeros after each tensor. Zero blocks cost 8 bytes of metadata (zero-block optimization), not data.

Benefits over CDC for model files:
- No rolling-hash scan overhead (was the dominant cost in the 177s Mistral-7B save time)
- Tensor blocks have stable identities across base and fine-tune — shape hasn't changed, so the block layout is the same. BLAKE3 dedup immediately identifies identical tensors.
- Padding zeros are free. Each block boundary is predictable.

---

## XOR delta compression (Phase 3)

> **Status:** in development. The algorithm is specified and partially implemented. Empirical compression ratios are `[UNTESTED]`.

For each tensor that exists in both the parent and child archives with the same shape and dtype:

```
delta_bytes[i] = parent_tensor[i] XOR child_tensor[i]  for all i
compressed_delta = zstd.compress(delta_bytes)
```

The compressed delta is stored as a data block. The block is tagged with `StorageMode::XorDelta` in the index. On read, the reader fetches both the compressed delta and the parent tensor, decompresses the delta, and XORs to reconstruct.

**Why zstd handles XOR deltas well:** fine-tuning perturbs each weight by a small amount relative to its full dynamic range. In float16 (2 bytes/weight), a small weight change produces XOR bytes that are near zero — the high-order bits of the mantissa and all exponent bits are often identical. zstd's entropy coding assigns short codes to frequent values (near-zero), achieving high compression.

**[UNTESTED: actual compression ratios on fine-tuned models with Hexz's implementation.]**

---

## Memory usage

Dedup table memory ≈ `(input_size / avg_block_size) × 48 bytes`.

| Input size | Avg block | Unique blocks | Memory |
|---|---|---|---|
| 1 GB | 64 KB | 16,384 | ~0.75 MB |
| 14 GB | 64 KB | 215,040 | ~10 MB |
| 100 GB | 64 KB | 1,638,400 | ~75 MB |

These are worst-case (no dedup). With a parent chain, only blocks not in the parent enter the dedup table.

Issue [#116](https://github.com/hexz-org/hexz/issues/116) tracks capping dedup map memory for very large packs.

---

## Limitations

**Encryption defeats dedup.** AES-GCM encryption produces ciphertext that looks random, breaking content-based matching. Encrypt at the filesystem level if you need both dedup and encryption.

**Parent chain is linear.** The parent path in the header is a single path. Branching checkpoint graphs (multiple fine-tunes from the same base) dedup correctly against the base, but two siblings do not dedup against each other unless one specifies the other as parent.

**Cross-archive dedup without parent.** Two separately-packed archives of the same model do not share blocks. This requires either using the parent chain, or a future shared external dedup index (unscheduled).

---

## See also

- [XOR Delta Compression](xor-delta-compression.md) — the delta algorithm in detail
- [Content-Defined Chunking](content-defined-chunking.md) — CDC internals
- [Architecture](architecture.md) — write path and storage modes
- [ADR-0003: BLAKE3 and FastCDC](../adr/0003-blake3-fastcdc-deduplication.md)
