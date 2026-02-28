# Hexz Examples

This directory contains examples demonstrating the key features and use cases of the
Hexz Python API, organized by topic.

## Getting Started

```bash
# Install from project root
pip install -e .

# Run the quickstart
python examples/quickstart.py

# Run any sub-example (add the examples/ prefix when running from repo root)
python examples/checkpoint/resnet_finetune_checkpoints.py
```

---

## `quickstart.py` — start here

A minimal end-to-end example: create a snapshot, compress it, read it back.

---

## `checkpoint/` — ML model checkpointing

Real fine-tuning workflows using `hexz.checkpoint`. Demonstrates cross-version
deduplication: only changed weights are stored; frozen layers are referenced
from the parent checkpoint without copying data.

| File | Description |
|------|-------------|
| `resnet_finetune_checkpoints.py` | ResNet-18 on CIFAR-10: pretrained → head fine-tune → layer4 fine-tune. Measures dedup savings and selective-load speedup. Requires `torchvision`. |
| `mistral_finetune_checkpoints.py` | Mistral-7B on Alpaca: pretrained → lm_head fine-tune → last-2-blocks fine-tune. Shows ~14 GB model stored as one full checkpoint plus two small deltas. Requires `transformers datasets`. |

---

## `arrays/` — NumPy / tensor array storage

Zero-copy array I/O using `hexz.write_array`, `hexz.read_array`, and `hexz.ArrayView`.

| File | Description |
|------|-------------|
| `video_frame_access.py` | Store raw video frames; pull individual frames or clips by byte-range without decompressing the whole file. |
| `medical_imaging_3d.py` | Store a 3D MRI/CT volume; slice arbitrary 2D planes via array indexing. |
| `vector_embeddings_lookup.py` | Use a Hexz snapshot as a high-performance read-only vector store. |
| `zero_copy_performance.py` | Benchmark zero-copy array loading vs. `pickle`. |

---

## `storage/` — General storage, deduplication, and cloud

| File | Description |
|------|-------------|
| `incremental_checkpoints.py` | Synthetic dedup demo: save a 100 MB blob, then a 95%-identical fine-tuned version; only the delta is stored. |
| `comprehensive_deduplication.py` | Deep dive into CDC chunking: linear chains, branching versions, multi-parent shards, and shift-resilience. |
| `global_deduplication.py` | Deduplicate a "hybrid" blob against two unrelated parent snapshots simultaneously. |
| `llm_weight_dedup.py` | Create a thin snapshot via `merge_overlay`: only stores blocks that differ from a base snapshot. |
| `configuration_profiles.py` | Use `hexz.build()` with preset profiles (`ml`, `archival`, …) and per-run overrides. |
| `distributed_loading.py` | Share a single `Reader` across multiple CPU worker processes via pickling. |
| `cloud_s3_streaming.py` | Stream data directly from S3 without downloading the full file. |
| `secure_signing.py` | Cryptographically sign and verify snapshots. Requires the `signing` feature. |

---

## `vm/` — VM and container workflows

| File | Description |
|------|-------------|
| `docker_layer_packing.py` | Pack Docker-style filesystem layers into Hexz snapshots; cross-layer deduplication eliminates shared OS libraries. |
| `fuse_mount_explorer.py` | Mount a snapshot as a virtual filesystem (Linux/macOS). Requires the `fuse` feature. |

---

## Notes

- Generated data (checkpoints, downloaded datasets) is written to `examples/.data/` and
  gitignored. Delete it with `rm -rf examples/.data/` to reclaim disk space.
- Examples that require optional features (`fuse`, `signing`) check for availability at
  runtime and skip gracefully if not compiled in.
- ML examples (`checkpoint/`) require `torch`; some also need `torchvision`,
  `transformers`, or `datasets`.
- Array examples require `numpy`.
