# Hexz Documentation

**Hexz** is a seekable, deduplicated archive format for ML model checkpoints. It reads safetensors and GGUF natively, chunks data at tensor boundaries, and stores fine-tuned models as XOR deltas against their base — so only what changed is written to disk.

## Quick Navigation

### I'm an ML Engineer
**Goal**: Store and load model checkpoints efficiently

1. Start: [Getting Started](tutorials/getting-started.md) (10 min)
2. Understand: [Why Hexz for ML](explanation/why-hexz-for-ml.md)
3. Deep dive: [XOR Delta Compression](explanation/xor-delta-compression.md)
4. Reference: [Python API](reference/python-api.md) · [CLI Reference](reference/cli-reference.md)

### I'm a Contributor
**Goal**: Understand architecture and contribute

1. Setup: [CONTRIBUTING.md](project-docs/CONTRIBUTING.md)
2. Architecture: [System Architecture](explanation/architecture.md)
3. Roadmap: [Development Roadmap](project-docs/ROADMAP.md)
4. Format: [File Format Spec](reference/file-format-spec.md)

---

## Documentation Structure

This documentation follows the [Diátaxis framework](https://diataxis.fr/):

| Quadrant | Purpose |
|---|---|
| **[Tutorials](tutorials/)** | Learn by doing — step-by-step from zero |
| **[How-To Guides](how-to/)** | Solve specific problems — practical recipes |
| **[Reference](reference/)** | Look up details — API and command specs |
| **[Explanation](explanation/)** | Understand concepts — design and rationale |

---

## Tutorials

- [Getting Started](tutorials/getting-started.md) — Pack your first model and load it back in 10 minutes

## How-To Guides

**ML Workflows**:
- [Store Fine-tuned Models](how-to/ml-workflows/store-finetuned-models.md) — Checkpoint chains, delta storage, parent references
- [Remote Access via S3](how-to/ml-workflows/setup-s3-streaming.md) — Load tensors on demand from object storage
- [Performance Tuning](how-to/performance-tuning.md) — Block size, compression level, CDC vs fixed chunking

## Reference

- [Python API Reference](reference/python-api.md) — Complete Python API (`hexz.checkpoint`, `hexz.open`, etc.)
- [CLI Command Reference](reference/cli-reference.md) — `hexz store`, `hexz extract`, `hexz diff`, etc.
- [Tensor Format Support](reference/tensor-formats.md) — Safetensors and GGUF format details
- [File Format Specification](reference/file-format-spec.md) — `.hxz` binary format
- [Compression Algorithms](reference/compression-algorithms.md) — lz4, zstd, XOR delta
- [Version Compatibility](reference/version-compatibility.md) — Python/PyTorch version matrix

## Explanation

- [System Architecture](explanation/architecture.md) — How Hexz works internally
- [Why Hexz for ML](explanation/why-hexz-for-ml.md) — Problem, solution, honest tradeoffs
- [XOR Delta Compression](explanation/xor-delta-compression.md) — The delta algorithm explained
- [Deduplication Deep Dive](explanation/deduplication-deep-dive.md) — BLAKE3, FastCDC, block dedup
- [Block vs File Compression](explanation/block-vs-file-compression.md) — Why block-level compression enables random access
- [Zero-Copy I/O](explanation/zero-copy-io.md) — Buffer protocol and memoryview paths

## ADRs

- [ADR-0001: Rust for Core Engine](adr/0001-rust-for-core-engine.md)
- [ADR-0002: Block-Level Compression](adr/0002-block-level-compression.md)
- [ADR-0003: BLAKE3 + FastCDC Deduplication](adr/0003-blake3-fastcdc-deduplication.md)
- [ADR-0004: Storage Backend Abstraction](adr/0004-storage-backend-abstraction.md)
- [ADR-0005: PyO3 Python Bindings](adr/0005-pyo3-python-bindings.md)

---

## Key Concepts

### .hxz archive

A `.hxz` file is an immutable, compressed archive with:
- **Block-level compression** — random access without full decompression
- **BLAKE3 deduplication** — identical blocks stored once, even across parent/child archives
- **Seekable 2-level index** — O(log N) lookup for any byte offset
- **Tensor manifest** — embedded map of tensor name → (offset, length, dtype, shape) for named-tensor access
- **Multiple backends** — local disk, S3, or HTTP with byte-range requests

### Tensor-level chunking

For safetensors and GGUF files, Hexz chunks at tensor boundaries rather than using content-defined chunking (CDC). The file header tells Hexz exactly where each tensor starts and ends — this is simpler than CDC, avoids the rolling-hash overhead, and means tensor-level deduplication is exact.

### XOR delta compression

When storing a fine-tuned model against its base, Hexz aligns tensors by name and XORs corresponding raw byte buffers. The result is sparse low-magnitude data that zstd compresses well. See [XOR Delta Compression](explanation/xor-delta-compression.md) for details.

> **Implementation status:** Tensor-level chunking (Phase 2) is complete. XOR delta compression (Phase 3) is in development. See [ROADMAP.md](project-docs/ROADMAP.md).

---

## Installation

```bash
pip install hexz           # Python package
cargo install hexz-cli     # CLI tool
```

## Community & Support

- **GitHub**: [hexz-org/hexz](https://github.com/hexz-org/hexz)
- **Issues**: [Report bugs or request features](https://github.com/hexz-org/hexz/issues)
- **Contributing**: See [CONTRIBUTING.md](project-docs/CONTRIBUTING.md)

## License

Apache License 2.0 — See [LICENSE](https://github.com/hexz-org/hexz/blob/main/LICENSE)
