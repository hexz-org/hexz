# XOR Delta Compression

This document explains how Hexz uses XOR-based delta compression to store fine-tuned model checkpoints efficiently against their base.

> **Implementation status:** XOR delta compression is Phase 3, currently in development. The algorithm is described here as it will work. Empirical compression results on real models are `[UNTESTED]`.

---

## The insight

Fine-tuning a neural network modifies the value of weights across many tensors, but it does not insert or delete bytes — every tensor in the fine-tuned model has the same shape, dtype, and byte length as in the base. The modifications are weight perturbations: small changes spread uniformly across the weight matrix.

XOR is the exact operation for capturing a perturbation between two same-size byte buffers: `delta = base XOR fine`. If the weights changed little, `delta` has low entropy — many bits are zero or close to zero. zstd compresses low-entropy data well.

This approach was validated at the algorithm level by Hachiuma et al. in "ZipLLM" (2024), which showed dramatic compression ratios on model weight deltas using this technique. Hexz's implementation generalizes it to arbitrary safetensors and GGUF files with delta chaining.

---

## Step-by-step algorithm

### Store path (`hexz store finetuned.safetensors finetuned.hxz --base base.hxz`)

1. **Parse headers.** Open both the source safetensors/GGUF file and the parent `.hxz` archive. Read their tensor manifests to get: tensor name → (byte offset, byte length, dtype, shape).

2. **Align tensors by name.** Build a map from tensor name to (source range, parent range). Three categories result:
   - **Identical** — tensor bytes are byte-for-byte equal (BLAKE3 hash match). Cost: zero bytes. The index entry points to the parent block.
   - **Changed** — tensor exists in both, shapes match, content differs. XOR delta is applied.
   - **New** — tensor exists only in the fine-tune (e.g., new adapter layers). Stored as-is.

3. **For each changed tensor:**
   - Read `base_bytes` from the parent archive (from its data blocks)
   - Read `fine_bytes` from the source safetensors file
   - Compute `delta[i] = base_bytes[i] XOR fine_bytes[i]` for each byte
   - Feed `delta` to the zstd compressor
   - Write the compressed delta as a data block and record it in the manifest with `storage: "xor_delta"`

4. **Pad to block boundary.** After each tensor, pad to the next `block_size` boundary with zero bytes. Zero blocks are stored as 8 bytes of metadata (the zero-block optimization in `SnapshotWriter`), not data. This alignment ensures that a tensor size change in one entry does not affect block boundaries for subsequent tensors.

5. **Write manifest.** Serialize the tensor manifest (name → offset, length, dtype, shape, storage mode) into the `metadata_offset` slot in the archive header.

### Load path (`ckpt.load("finetuned.hxz", keys=["lm_head.weight"])`)

1. Read the tensor manifest.
2. For each requested tensor:
   - If `storage == "xor_delta"`: read `compressed_delta` from this archive, decompress it, read `base_bytes` from the parent archive, compute `fine_bytes[i] = base_bytes[i] XOR compressed_delta[i]`. Return `fine_bytes`.
   - If `storage == "raw"` or `storage == "dedup_ref"`: read directly.
3. Return a dict of `{name: tensor}`.

---

## Why tensor boundaries, not CDC

Content-defined chunking (FastCDC) works by scanning a byte stream with a rolling hash and cutting at content-dependent boundaries. For model files:

- The safetensors and GGUF headers already tell you exactly where every tensor starts and ends. There is no need to scan.
- Tensor boundaries never shift between a base model and its fine-tune — the shapes are identical. This makes tensor-boundary chunking strictly better than CDC for this use case: CDC might split a tensor across two chunks, while tensor-boundary chunking never does.
- Avoiding the CDC scan eliminates the rolling-hash overhead over the entire file (which was the main cause of the 177s save time on Mistral-7B).

---

## Why XOR, not binary diff

Binary diff tools (bsdiff, xdelta, zstd `--patch-from`) work well when the delta is structured: insertions, deletions, and moves of byte sequences. Model weight updates are not structured this way — fine-tuning changes the value of weights uniformly across all parameters without inserting or deleting bytes.

For this pattern:
- Binary diff produces large, complex patch files because it cannot find the insertion/deletion structure that doesn't exist
- XOR produces a dense byte array of the same size as the tensors, with each byte being the XOR of the corresponding weight bytes — which is exactly the magnitude of the change
- zstd then compresses the XOR result; if weights changed little, the XOR bytes cluster near zero and compress well

---

## SIMD acceleration

XOR of two large byte arrays is one of the most SIMD-friendly operations possible. Each iteration processes 256 bits (32 bytes) independently:

```
for i in 0..len/32 {
    result[i] = base[i] XOR fine[i]   // AVX2: one _mm256_xor_si256 instruction
}
```

A 14 GB tensor pair can be XOR'd at memory bandwidth speed. On a modern CPU with 50 GB/s memory bandwidth, this takes roughly 0.3 seconds. The zstd compression step dominates total time, not the XOR.

Hexz uses `portable_simd` (Rust nightly) or falls back to scalar XOR, which the compiler auto-vectorizes on most targets.

---

## Shape mismatch handling

XOR delta requires that the base and fine-tuned tensor have the same shape and dtype. If they differ (e.g., quantization format change, architectural modification), Hexz falls back to storing the tensor raw with no delta. A warning is emitted:

```
Warning: tensor layers.31.mlp.gate_proj.weight: shape mismatch ([4096, 14336] vs [4096, 11008])
         Stored raw (no delta). Consider updating --base to a compatible checkpoint.
```

---

## Compression ratio expectations

The actual compression ratio depends on:
1. What fraction of weights were modified (LoRA modifies only adapter weights; full fine-tune modifies everything)
2. How large the weight changes are (learning rate, number of steps, dataset)
3. The dtype (float32 has 4× more bits than int8 to perturb)

**[UNTESTED: empirical compression ratios on real fine-tuned models with Hexz's XOR delta implementation.]**

The ZipLLM paper reports significant compression ratios on their implementation using the same algorithmic approach. Hexz benchmarks on real models will be published once Phase 3 is complete.

---

## See also

- [Deduplication Deep Dive](deduplication-deep-dive.md) — BLAKE3 block dedup
- [Tensor Format Support](../reference/tensor-formats.md) — safetensors and GGUF parsers
- [Architecture](architecture.md) — write path and manifest storage
- [ROADMAP.md](../project-docs/ROADMAP.md) — Phase 3 implementation plan
