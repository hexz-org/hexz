# Hexz Documentation

**Hexz** is a seekable, block-compressed archive format with content deduplication, written in Rust with Python bindings (PyO3). It stores large binary data compressed, and supports reading any byte range without decompressing the whole file.

## Quick Navigation by Role

### I'm an ML Engineer
**Goal**: Store and load model checkpoints cheaply

1. Start: [Getting Started Tutorial](tutorials/getting-started.md) (10 min)
2. Explore: [Deduplication Deep Dive](explanation/deduplication-deep-dive.md)
3. Optimize: [Fine-tuning Workflows](how-to/ml-workflows/streaming-best-practices.md)
4. Reference: [Python API](reference/python-api.md)

**Why Hexz?** [Understand the storage savings](explanation/why-hexz-for-ml.md)

### I'm a Systems Engineer / VM User
**Goal**: Manage VM images and snapshots efficiently

1. Start: [Getting Started Tutorial](tutorials/getting-started.md) (10 min)
2. Boot: [Booting Your First VM](tutorials/booting-your-first-vm.md) (15 min)
3. Manage: [Create VM Snapshots](how-to/vm-management/create-vm-snapshots.md)
4. Reference: [CLI Commands](reference/cli-reference.md)

### I'm a Contributor
**Goal**: Understand architecture and contribute

1. Setup: [CONTRIBUTING.md](project-docs/CONTRIBUTING.md)
2. Architecture: [System Architecture](explanation/architecture.md)
3. Roadmap: [Development Roadmap](project-docs/ROADMAP.md)
4. Code: [GitHub Repository](https://github.com/Alethic-Systems/hexz)

---

## Documentation Structure

This documentation follows the [Diátaxis framework](https://diataxis.fr/), organizing content into four quadrants:

### Tutorials (Learning-Oriented)
*Learn by doing — step-by-step lessons for beginners*

- [Getting Started with Hexz](tutorials/getting-started.md) — Your first snapshot in 10 minutes
- [Model Checkpoint Dedup](examples/checkpoint_pivot.py) — (Example Script) Thin snapshots for fine-tuned models
- [Booting Your First VM](tutorials/booting-your-first-vm.md) — Boot an OS from a snapshot

### How-To Guides (Goal-Oriented)
*Solve specific problems — practical recipes for common tasks*

**ML Workflows**:
- [Checkpoint Storage](how-to/ml-workflows/streaming-best-practices.md) — Optimizing storage for model versions
- [Setup S3 Streaming](how-to/ml-workflows/setup-s3-streaming.md) — Remote checkpoint access

**VM Management**:
- [Create VM Snapshots](how-to/vm-management/create-vm-snapshots.md)
- [Boot VM from Snapshot](how-to/vm-management/boot-vm-from-snapshot.md)
- [Commit Overlay Changes](how-to/vm-management/commit-overlay-changes.md)

### Reference (Information-Oriented)
*Look up details — technical specifications and API docs*

- [Python API Reference](reference/python-api.md) — Complete Python API
- [CLI Command Reference](reference/cli-reference.md) — All CLI commands and flags
- [File Format Specification](reference/file-format-spec.md) — `.hxz` file format details
- [Version Compatibility](reference/version-compatibility.md) — Python/PyTorch versions

### Explanation (Understanding-Oriented)
*Understand concepts — design rationale and deep dives*

- [System Architecture](explanation/architecture.md) — How Hexz works internally
- [Why Hexz for ML](explanation/why-hexz-for-ml.md) — Problem/solution explanation
- [Deduplication Deep Dive](explanation/deduplication-deep-dive.md) — FastCDC and BLAKE3
- [Zero-Copy I/O](explanation/zero-copy-io.md) — Performance internals

---

## Key Concepts

### What is a Snapshot?

A **snapshot** (`.hxz` file) is an immutable, compressed archive with:
- **Block-level compression**: Random access without full decompression
- **Content-defined chunking**: Deduplication across versions and files
- **Seekable index**: O(log N) lookup for any offset
- **Multiple backends**: Works on local disk, S3, or HTTP

### Core Features

- **Deduplication**: CDC-based dedup across checkpoint chains — only changed blocks are stored.
- **Random Access**: Read any byte range without downloading or decompressing the whole file.
- **Buffer protocol**: Direct loading into NumPy and PyTorch buffers via Python's buffer protocol.
- **Remote backends**: Byte-range fetching from S3 or HTTP — only the blocks you need.

---

## Installation

### Python Package (Recommended)

```bash
pip install hexz
```

### CLI Tool

```bash
cargo install hexz-cli
```

---

## Quick Examples

### Store a Fine-tuned Model

```python
import hexz

# Save model while deduplicating against base version
with hexz.Writer("finetuned-v2.hxz", parent="base-model.hxz", cdc=True) as writer:
    writer.add_bytes(model_weights)
```

### Fetch One Layer from S3

```python
import hexz
import torch

# Random access over S3 - only download the requested bytes
with hexz.open("s3://bucket/llama-70b.hxz") as reader:
    layer_raw = reader.read(length, offset=layer_offset)
    layer = torch.frombuffer(layer_raw, dtype=torch.float32)
```

---

## Community & Support

- **GitHub**: [Alethic-Systems/hexz](https://github.com/Alethic-Systems/hexz)
- **Issues**: [Report bugs or request features](https://github.com/Alethic-Systems/hexz/issues)
- **Contributing**: See [CONTRIBUTING.md](project-docs/CONTRIBUTING.md)

---

## License

Apache License 2.0 — See [LICENSE](https://github.com/Alethic-Systems/hexz/blob/main/LICENSE)