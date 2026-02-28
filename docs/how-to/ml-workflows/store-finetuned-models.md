# Store Fine-tuned Models

**Goal:** Set up a checkpoint chain where fine-tuned models store only their delta against the base, with random access to individual tensors.

---

## Prerequisites

- Hexz installed: `pip install hexz` and `cargo install hexz-cli`
- A base model in safetensors or GGUF format
- At least one fine-tuned version

---

## Step 1 — Pack the base model

```bash
hexz store base-model.safetensors base-model.hxz --compression zstd
```

Or from Python:
```python
import hexz.checkpoint as ckpt
ckpt.convert("base-model.safetensors", "base-model.hxz", compression="zstd")
```

This is the anchor of your checkpoint chain. Every fine-tune will reference it.

---

## Step 2 — Pack a fine-tune as a delta

```bash
hexz store finetuned-v1.safetensors finetuned-v1.hxz --base base-model.hxz
```

Hexz aligns tensors by name between `base-model.hxz` and `finetuned-v1.safetensors`. Byte-identical tensors are referenced from the parent and not re-stored. Changed tensors are stored compressed. (XOR delta compression — Phase 3 — will improve savings further once available.)

To verify what was stored:
```bash
hexz inspect finetuned-v1.hxz
hexz diff base-model.hxz finetuned-v1.hxz
```

---

## Step 3 — Chain multiple fine-tunes

You can chain directly:
```bash
# v2 references v1 as parent
hexz store finetuned-v2.safetensors finetuned-v2.hxz --base finetuned-v1.hxz
```

Or all reference the same base (better for independent experiments):
```bash
hexz store lora-run-a.safetensors lora-run-a.hxz --base base-model.hxz
hexz store lora-run-b.safetensors lora-run-b.hxz --base base-model.hxz
hexz store lora-run-c.safetensors lora-run-c.hxz --base base-model.hxz
```

View the chain:
```bash
hexz ls ./checkpoints/
```

---

## Step 4 — Load in Python

```python
import hexz.checkpoint as ckpt

# Load full model
state = ckpt.load("finetuned-v1.hxz", device="cuda")

# Load specific tensors only — reads only those blocks
state = ckpt.load("finetuned-v1.hxz", keys=["lm_head.weight", "embed_tokens.weight"])

# Inspect available tensors without loading
manifest = ckpt.manifest("finetuned-v1.hxz")
for name, info in manifest.items():
    print(f"{name}: {info['shape']} {info['dtype']}")
```

Loading is transparent regardless of storage mode — Hexz resolves delta chains automatically.

---

## Step 5 — Save a PyTorch state dict directly

If you're fine-tuning with PyTorch and want to save checkpoints directly:

```python
import hexz.checkpoint as ckpt

# During training
ckpt.save(
    model.state_dict(),
    f"checkpoints/step-{step}.hxz",
    parent="base-model.hxz",
    metadata={"step": step, "loss": loss.item(), "lr": lr},
)
```

Each checkpoint is saved without CDC (fixed block chunking by default), so the save is fast. The block-boundary alignment in `save()` ensures that tensors start at block boundaries, maximizing dedup with the parent.

---

## Step 6 — Export back to safetensors

```bash
hexz extract finetuned-v1.hxz finetuned-v1-out.safetensors
```

Or from Python:
```python
ckpt.extract("finetuned-v1.hxz", "finetuned-v1-out.safetensors")
```

Round-trip fidelity: the exported safetensors file is byte-for-byte identical in tensor content to the original. The safetensors header is reconstructed from the manifest stored in the archive.

---

## Remote storage (S3)

Upload archives to S3 after packing:
```bash
aws s3 cp base-model.hxz s3://my-bucket/models/base-model.hxz
aws s3 cp finetuned-v1.hxz s3://my-bucket/models/finetuned-v1.hxz
```

Load over S3 — only the blocks for the requested tensors are downloaded:
```python
import hexz.checkpoint as ckpt

# The parent archive is also fetched on demand as needed
state = ckpt.load(
    "s3://my-bucket/models/finetuned-v1.hxz",
    keys=["lm_head.weight"],
)
```

See [Remote Access via S3](setup-s3-streaming.md) for cache sizing and region configuration.

---

## Attach metadata for experiment tracking

```python
ckpt.save(
    model.state_dict(),
    "run-42.hxz",
    parent="base-model.hxz",
    metadata={
        "run_id": "run-42",
        "dataset": "openhermes-2.5",
        "learning_rate": 2e-5,
        "epochs": 3,
        "final_loss": 0.041,
        "base_model": "mistral-7b-v0.1",
    },
)

# Retrieve later
import hexz
meta = hexz.inspect("run-42.hxz")
print(meta.user_metadata)
```

---

## Checklist

- [ ] Base model packed with `--compression zstd` (better ratio for model weights)
- [ ] Fine-tunes packed with `--base base-model.hxz`
- [ ] `hexz diff` run to verify dedup is happening
- [ ] `hexz ls` shows expected chain structure
- [ ] `ckpt.load(..., keys=[...])` tested to confirm selective loading works
- [ ] Archives uploaded to S3 for remote access
- [ ] Metadata attached for experiment tracking

---

## See also

- [Getting Started](../../tutorials/getting-started.md)
- [XOR Delta Compression](../../explanation/xor-delta-compression.md)
- [Remote Access via S3](setup-s3-streaming.md)
- [Python API Reference](../../reference/python-api.md)
- [CLI Reference](../../reference/cli-reference.md)
