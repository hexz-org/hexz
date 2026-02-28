# Tensor Format Support

Reference for the tensor file formats that Hexz reads and writes natively.

> **Implementation status:** Safetensors and GGUF parsing land in v0.6.0. See [ROADMAP.md](../project-docs/ROADMAP.md).

---

## Supported formats

| Format | Read | Write (export) | Notes |
|---|---|---|---|
| `.safetensors` | Yes (v0.6.0) | Yes (v0.6.0) | Primary format; HuggingFace default |
| `.gguf` | Yes (v0.6.0) | Planned (v1.x) | llama.cpp quantized models |
| PyTorch `.pt` / `.pth` | Via `torch.load` bridge | Via `hexz.checkpoint.save` | Requires PyTorch installed |
| NumPy `.npy` / `.npz` | Yes | Yes | Via `hexz.write_array` / `hexz.read_array` |

---

## Safetensors

### Wire format

```
[8 bytes, LE u64]  header_length
[header_length bytes, UTF-8]  header_json
[remaining bytes]  tensor data
```

`tensor data` begins at byte offset `8 + header_length`. All `data_offsets` in the header JSON are relative to this position.

### Header JSON schema

```json
{
  "__metadata__": {
    "model_type": "mistral",
    "format": "pt"
  },
  "embed_tokens.weight": {
    "dtype": "BF16",
    "shape": [32000, 4096],
    "data_offsets": [0, 268435456]
  },
  "layers.0.self_attn.q_proj.weight": {
    "dtype": "BF16",
    "shape": [4096, 4096],
    "data_offsets": [268435456, 301989888]
  }
}
```

`data_offsets` is `[start, end)` — a half-open range. `end - start` must equal the product of `shape` elements multiplied by the dtype byte width.

### Dtype encoding

| Safetensors dtype | Bytes/element |
|---|---|
| `F64` | 8 |
| `F32` | 4 |
| `BF16` | 2 |
| `F16` | 2 |
| `I64` | 8 |
| `I32` | 4 |
| `I16` | 2 |
| `I8` | 1 |
| `U8` | 1 |
| `BOOL` | 1 |
| `F8_E4M3` | 1 |
| `F8_E5M2` | 1 |

### How Hexz uses safetensors

1. Read 8-byte header length.
2. Read `header_length` bytes and parse as JSON using `serde_json` with an `IndexMap` (order-preserving).
3. Record `data_start = 8 + header_length` (absolute byte offset where tensor data begins).
4. For each tensor in declaration order, compute absolute `data_start` and `data_end`.
5. Chunk at tensor boundaries — write each tensor's bytes in `block_size` chunks, padded to boundary.
6. Store the original header JSON in the manifest (`safetensors_header` field) for lossless round-trip.

**Export:** to reconstruct a safetensors file, Hexz reads the stored `safetensors_header`, recomputes `data_offsets` for sequential output (tensors packed with no gaps), writes `[len][json][tensor data...]`.

### Python API

```python
import hexz.checkpoint as ckpt

# Pack (no PyTorch needed)
ckpt.convert("model.safetensors", "model.hxz")
ckpt.convert("finetuned.safetensors", "finetuned.hxz", base="model.hxz")

# Export
ckpt.extract("finetuned.hxz", "finetuned-out.safetensors")

# Extract single tensor (raw bytes, no header)
ckpt.extract("finetuned.hxz", tensor="lm_head.weight")
```

### CLI

```bash
hexz store model.safetensors model.hxz [--base parent.hxz] [--compression zstd]
hexz extract model.hxz model-out.safetensors
hexz extract model.hxz --tensor lm_head.weight
```

---

## GGUF

### Wire format

```
[4 bytes]  magic: "GGUF" (0x47 0x47 0x55 0x46)
[4 bytes, LE u32]  version (3 for GGUF v3)
[8 bytes, LE u64]  tensor_count
[8 bytes, LE u64]  metadata_kv_count
[metadata_kv_count × KV pairs]  metadata
[tensor_count × TensorInfo]  tensor info
[padding to 32-byte alignment]
[tensor data]
```

### TensorInfo structure

Each `TensorInfo` contains:
- Name: `u64` length + UTF-8 string
- `n_dimensions: u32`
- `dims: [u64; n_dimensions]` (shape, row-major)
- `type: u32` (quantization type enum)
- `offset: u64` (byte offset relative to start of tensor data section)

### Tensor types (quantization)

| GGUF type | Bytes/element | Notes |
|---|---|---|
| `F32` | 4 | |
| `F16` | 2 | |
| `BF16` | 2 | |
| `Q4_0` | 0.5 (4 bits) | Quantized |
| `Q4_K_M` | ~0.55 | Quantized, mixed |
| `Q8_0` | 1 | Quantized |
| ... | | Many quantization types |

### How Hexz uses GGUF

Same approach as safetensors: parse the tensor info array to get name → byte range, then chunk at tensor boundaries. The GGUF header is stored verbatim in the manifest for round-trip fidelity.

> **Note:** XOR delta against a GGUF parent requires that both files use the same quantization type for each tensor. Mixing quantization levels (e.g., Q4 base and Q8 fine-tune) triggers the shape-mismatch path and stores raw.

---

## Internal `.hxz` tensor manifest

The tensor manifest is stored as a msgpack blob at the `metadata_offset` location in the `.hxz` header. Schema:

```json
{
  "format": "safetensors",
  "version": "1",
  "safetensors_header": "<original safetensors JSON, verbatim>",
  "tensors": {
    "embed_tokens.weight": {
      "offset": 0,
      "length": 268435456,
      "dtype": "BF16",
      "shape": [32000, 4096],
      "storage": "raw"
    },
    "lm_head.weight": {
      "offset": 268500992,
      "length": 268435456,
      "dtype": "BF16",
      "shape": [32000, 4096],
      "storage": "xor_delta"
    }
  }
}
```

**Fields:**
- `offset` — logical byte offset in the archive's virtual address space
- `length` — byte length of the tensor data (before padding)
- `dtype` — dtype string as it appears in the source format
- `shape` — shape as `[u64]`
- `storage` — one of `"raw"`, `"dedup_ref"`, `"xor_delta"`, `"zero"`

The `offset`/`length` pair is passed to `reader.read(length, offset=offset)` to retrieve tensor bytes. Padding zeros between tensors are not included in `length`.

---

## See also

- [XOR Delta Compression](../explanation/xor-delta-compression.md)
- [Architecture](../explanation/architecture.md)
- [CLI Reference](cli-reference.md)
- [Python API Reference](python-api.md)
