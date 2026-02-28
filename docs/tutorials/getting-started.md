# Getting Started with Hexz

**Time:** ~10 minutes
**What you'll do:** Pack a safetensors model, inspect it, store a fine-tune as a delta, and load tensors selectively.

---

## Prerequisites

- Linux or macOS
- Python 3.8+
- A `.safetensors` file (any HuggingFace model works)

---

## Install

```bash
pip install hexz
cargo install hexz-cli   # CLI tool
```

Verify:
```bash
python -c "import hexz; print(hexz.__version__)"
hexz --version
```

---

## Step 1 — Pack a model

```bash
hexz store base-model.safetensors base-model.hxz
```

```
Packing base-model.safetensors → base-model.hxz
  Tensors: 362
  Total:   13.8 GB
Done.
```

Inspect it:
```bash
hexz inspect base-model.hxz
```

```
Archive:     base-model.hxz
Compression: zstd (level 3)
Tensors (362):
  embed_tokens.weight    BF16  [32000, 4096]  256.0 MB
  layers.0.self_attn.q_proj.weight  BF16  [4096, 4096]  32.0 MB
  ...
```

---

## Step 2 — Store a fine-tune as a delta

```bash
hexz store finetuned.safetensors finetuned.hxz --base base-model.hxz
```

Hexz aligns tensors by name between the base and fine-tune. Byte-identical blocks are referenced from the parent and not re-stored. Changed blocks are compressed independently.

> **Note:** XOR delta compression (Phase 3) is in development. The current build stores changed blocks as-is; the storage savings shown will improve significantly once XOR delta lands. See [ROADMAP.md](../project-docs/ROADMAP.md).

Compare the two:
```bash
hexz diff base-model.hxz finetuned.hxz
```

---

## Step 3 — Load tensors in Python

```python
import hexz.checkpoint as ckpt

# Inspect without loading
manifest = ckpt.manifest("finetuned.hxz")
for name, info in manifest.items():
    print(name, info["shape"], info["dtype"])

# Load all tensors
state = ckpt.load("finetuned.hxz")

# Load only specific tensors — reads only those blocks
state = ckpt.load("finetuned.hxz", keys=["lm_head.weight", "embed_tokens.weight"])
```

The `keys=` parameter uses the tensor manifest to find the exact byte range for each tensor. Only those blocks are read and decompressed — the rest of the file is not touched.

---

## Step 4 — Convert from safetensors (no PyTorch needed)

```python
import hexz.checkpoint as ckpt

# Convert — reads tensor bytes directly from the safetensors file
ckpt.convert("base-model.safetensors", "base-model.hxz")

# Convert a fine-tune with delta against the base
ckpt.convert("finetuned.safetensors", "finetuned.hxz", base="base-model.hxz")

# Export back to safetensors
ckpt.extract("finetuned.hxz", "finetuned-out.safetensors")
```

`ckpt.convert()` does not require PyTorch — it reads raw bytes from the safetensors file using Hexz's native parser.

---

## Step 5 — Save a PyTorch state dict directly

```python
import torch
import hexz.checkpoint as ckpt

model = ...  # your model

# Save
ckpt.save(model.state_dict(), "finetuned.hxz", parent="base-model.hxz")

# Load back
state = ckpt.load("finetuned.hxz", device="cuda")
model.load_state_dict(state)
```

---

## What to read next

- [Store Fine-tuned Models](../how-to/ml-workflows/store-finetuned-models.md) — checkpoint chains, S3, metadata
- [XOR Delta Compression](../explanation/xor-delta-compression.md) — how the delta algorithm works
- [Python API Reference](../reference/python-api.md)
- [CLI Reference](../reference/cli-reference.md)
