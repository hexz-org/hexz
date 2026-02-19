# Hexz: Efficient Model Checkpoint Storage

[![Documentation](https://img.shields.io/badge/docs-latest-blue.svg)](https://hexz-org.github.io/hexz/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE-APACHE)

Hexz is a high-performance storage engine designed for **ML model checkpoints**. It uses content-defined chunking (CDC) and a two-level index to enable massive storage savings and instant random access for large model weights (fine-tuning, LoRA sweeps, RLHF).

**Store 100 fine-tuned checkpoints for the cost of 6.**

---

## Why Hexz?

- **Extreme Deduplication** — CDC-based dedup across checkpoint chains means only changed weights are stored. A 14GB model fine-tuned 50 times costs ~20GB, not 700GB.
- **Instant Random Access** — Read specific tensors from a 100GB checkpoint (e.g., a single layer) without downloading or decompressing the whole file. O(log N) lookup.
- **Thin Snapshots** — Child checkpoints reference their parent and only store deltas. Perfect for iterative fine-tuning and audit trails.
- **Zero-Copy I/O** — Direct loading into NumPy and PyTorch buffers via the Python buffer protocol. Released GIL during I/O.
- **Cloud Native** — Built-in S3 and HTTP backends with efficient byte-range fetching. Download only the blocks you need.
- **Single File** — Self-contained snapshots with data, index, and metadata. No running daemons or complex infrastructure.

---

## The Core Primitive

**Read any tensor, from any version, without downloading the whole file, from a single archive on S3.**

### Quick Start

```python
import hexz
import torch

# Load a specific weight from a 14GB checkpoint over S3
# Only the required blocks are downloaded.
with hexz.open("s3://models/llama-7b.hxz") as reader:
    # Get uncompressed size info
    meta = reader.metadata
    print(f"Model size: {meta.primary_size / 1e9:.1f} GB")
    
    # Fetch just the layer we need (random access)
    weights_raw = reader.read(length=1024*1024, offset=4096)
    weights = torch.frombuffer(weights_raw, dtype=torch.float32)

# Create a deduplicated checkpoint (references parent)
with hexz.Writer("finetuned.hxz", packing="tight") as writer:
    writer.merge_overlay(base="base_model.hxz", overlay="delta.bin", thin=True)
```

---

## What it is not

- **Not a training data pipeline**: WebDataset or StreamingDataset are better for sequential sample streaming. Hexz is optimized for model weights and random-access blobs.
- **Not a Python serializer**: It stores raw byte blobs (tensors), not arbitrary pickled Python objects.
- **Not a platform**: Hexz is a file format and library, not a replacement for Weights & Biases or MLflow.

---

## Validated Engine Performance

*Benchmarks run on Intel i7-14700K, NVMe SSD, Linux.*

| Operation | Throughput |
|---|---|
| LZ4 decompress | 32.1 GB/s |
| LZ4 compress | 23.6 GB/s |
| Pack LZ4 + CDC | 1.9 GB/s |
| Sequential read | 9.0 GB/s |
| Random access (cold) | 6.6 µs |
| Random access (warm) | 174 ns |

**Deduplication Efficiency:**

50MB base file + 50MB shifted version (1KB inserted at start):

| Method | Combined size | Dedup of shifted data |
|---|---|---|
| Fixed-size blocks | 100.4 MB | 0% |
| CDC blocks | 54.0 MB | **92.4%** |

---

## Installation

```bash
# Python library
pip install hexz

# CLI tool
cargo install hexz-cli
```

---

## License

Licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.