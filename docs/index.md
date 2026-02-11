# Strata Documentation

**Strata** is a high-performance, seekable compressed filesystem and data streaming engine designed for two primary use cases:

1. **AI/ML Data Loading** — Stream massive datasets from S3 directly to GPU, bypassing Python GIL
2. **VM Management** — Boot entire operating systems over network in milliseconds

Built in Rust with Python bindings (PyO3), Strata provides random access to compressed data with sub-millisecond latency.

## Quick Navigation by Role

### I'm an ML Engineer
**Goal**: Stream training data efficiently

1. Start: [Getting Started Tutorial](tutorials/getting-started.md) (10 min)
2. Build: [Your First ML Pipeline](tutorials/first-ml-pipeline.md) (20 min)
3. Deploy: [Setup S3 Streaming](how-to/ml-workflows/setup-s3-streaming.md)
4. Optimize: [Optimize PyTorch DataLoader](how-to/ml-workflows/optimize-pytorch-dataloader.md)
5. Reference: [Python API](reference/python-api.md)

**Why Strata?** [Understand the problem we solve](explanation/why-strata-for-ml.md)

### I'm a Systems Engineer / VM User
**Goal**: Manage VM images efficiently

1. Start: [Getting Started Tutorial](tutorials/getting-started.md) (10 min)
2. Boot: [Booting Your First VM](tutorials/booting-your-first-vm.md) (15 min)
3. Manage: [Create VM Snapshots](how-to/vm-management/create-vm-snapshots.md)
4. Network: [Setup VM Networking](how-to/vm-management/setup-vm-networking.md)
5. Reference: [CLI Commands](reference/cli-reference.md)

### I'm a Contributor
**Goal**: Understand architecture and contribute

1. Setup: [CONTRIBUTING.md](project-docs/CONTRIBUTING.md)
2. Architecture: [System Architecture](explanation/architecture.md)
3. Decisions: See ADR section below for architectural decisions
4. Roadmap: [Development Roadmap](project-docs/ROADMAP.md)
5. Code: [GitHub Repository](https://github.com/Alethic-Systems/strata)

### I Need Quick Help
**Goal**: Solve specific problems now

- [Troubleshooting Guide](how-to/troubleshooting.md)
- [Performance Tuning](how-to/performance-tuning.md)
- [Installation Issues](how-to/cli-usage/install-strata.md)
- [CLI Reference](reference/cli-reference.md)
- [Python API Reference](reference/python-api.md)

---

## Documentation Structure

This documentation follows the [Diátaxis framework](https://diataxis.fr/), organizing content into four quadrants:

### Tutorials (Learning-Oriented)
*Learn by doing — step-by-step lessons for beginners*

- [Getting Started with Strata](tutorials/getting-started.md) — Your first snapshot in 10 minutes
- [Your First ML Pipeline](tutorials/first-ml-pipeline.md) — Stream images to PyTorch
- [Booting Your First VM](tutorials/booting-your-first-vm.md) — Boot an OS from a snapshot
- [Understanding Compression](tutorials/understanding-compression.md) — Hands-on compression concepts

### How-To Guides (Goal-Oriented)
*Solve specific problems — practical recipes for common tasks*

**ML Workflows**:
- [Setup S3 Streaming](how-to/ml-workflows/setup-s3-streaming.md)
- [Optimize PyTorch DataLoader](how-to/ml-workflows/optimize-pytorch-dataloader.md)
- [Migrate from WebDataset](how-to/ml-workflows/migrate-from-webdataset.md)
- [Streaming Best Practices](how-to/ml-workflows/streaming-best-practices.md)

**VM Management**:
- [Create VM Snapshots](how-to/vm-management/create-vm-snapshots.md)
- [Setup VM Networking](how-to/vm-management/setup-vm-networking.md)
- [Boot VM from Snapshot](how-to/vm-management/boot-vm-from-snapshot.md)
- [Commit Overlay Changes](how-to/vm-management/commit-overlay-changes.md)

**CLI Usage**:
- [Pack Datasets](how-to/cli-usage/pack-datasets.md)
- [Install Strata](how-to/cli-usage/install-strata.md)
- [Verify Signatures](how-to/cli-usage/verify-signatures.md)

**General**:
- [Troubleshooting](how-to/troubleshooting.md)
- [Performance Tuning](how-to/performance-tuning.md)

### Reference (Information-Oriented)
*Look up details — technical specifications and API docs*

- [Python API Reference](reference/python-api.md) — Complete Python API
- [CLI Command Reference](reference/cli-reference.md) — All CLI commands and flags
- [File Format Specification](reference/file-format-spec.md) — `.st` file format details
- [Compression Algorithms](reference/compression-algorithms.md) — LZ4 vs Zstd comparison
- [Configuration Options](reference/configuration.md) — Environment variables and config
- [Version Compatibility](reference/version-compatibility.md) — Python/PyTorch versions

### Explanation (Understanding-Oriented)
*Understand concepts — design rationale and deep dives*

- [System Architecture](explanation/architecture.md) — How Strata works internally
- [Why Strata for ML](explanation/why-strata-for-ml.md) — Problem/solution explanation
- [Deduplication Deep Dive](explanation/deduplication-deep-dive.md) — FastCDC and BLAKE3
- [Compression Strategy](explanation/compression-strategy.md) — Block vs file compression
- [Content-Defined Chunking](explanation/content-defined-chunking.md) — CDC concepts
- [Zero-Copy I/O](explanation/zero-copy-io.md) — Performance internals
- [Storage Backend Design](explanation/storage-backend-design.md) — S3/HTTP/Local abstraction
- [Block vs File Compression](explanation/block-vs-file-compression.md) — Compression trade-offs

### Architectural Decision Records (ADRs)
*Design decisions — why we built it this way*

- [ADR-0001: Rust for Core Engine](adr/0001-rust-for-core-engine.md)
- [ADR-0002: Block-Level Compression](adr/0002-block-level-compression.md)
- [ADR-0003: BLAKE3 and FastCDC Deduplication](adr/0003-blake3-fastcdc-deduplication.md)
- [ADR-0004: Storage Backend Abstraction](adr/0004-storage-backend-abstraction.md)
- [ADR-0005: PyO3 for Python Bindings](adr/0005-pyo3-python-bindings.md)

### Project Documentation
*Project meta — contributing, roadmap, releases*

- [Contributing Guide](project-docs/CONTRIBUTING.md) — Setup, workflow, PR checklist
- [Development Roadmap](project-docs/ROADMAP.md) — Planned features
- [Benchmarks](project-docs/BENCHMARKS.md) — Performance measurements
- [Release Notes v0.1.0-alpha](project-docs/release/RELEASE_NOTES_v0.1.0-alpha.md) — Latest release

---

## Key Concepts

### What is a Snapshot?

A **snapshot** (`.st` file) is an immutable, compressed archive with:
- **Block-level compression**: Random access without full decompression
- **Content-defined chunking**: Deduplication across versions
- **Seekable index**: O(log N) lookup for any offset
- **Multiple backends**: Works on local disk, S3, or HTTP

Think of it as "tar.gz with random access" or "VM disk image but compressed".

### Core Features

- **Random Access**: Seek to any byte in O(log N) time, decompress only needed blocks
- **Streaming**: Train from S3 without downloading entire dataset
- **Deduplication**: 40-70% storage savings on typical ML datasets
- **Zero-Copy I/O**: Direct memory mapping to NumPy/PyTorch tensors
- **Dual Use**: Same engine for ML datasets and VM images
- **Storage Agnostic**: Local files, S3, HTTP/HTTPS

### Use Cases

**ML Training**:
```python
import strata
import torch

# Stream from S3
dataset = strata.open("s3://bucket/imagenet.st")
loader = torch.utils.data.DataLoader(dataset, batch_size=32)

for batch in loader:
    train(batch)  # Data streams directly to GPU
```

**VM Boot**:
```bash
# Boot Ubuntu directly from compressed snapshot
strata vm boot ubuntu.st --ram 4G --net
```

---

## Installation

### Python Package (ML Use Case)

```bash
# From source
git clone https://github.com/Alethic-Systems/strata.git
cd strata
make develop

# Verify
python -c "import strata; print(strata.__version__)"
```

### CLI Tool (VM Use Case)

```bash
# From source
git clone https://github.com/Alethic-Systems/strata.git
cd strata
make rust

# Verify
./target/release/strata --version
```

See [Installation Guide](how-to/cli-usage/install-strata.md) for detailed instructions.

---

## Quick Examples

### Pack a Dataset

**Python**:
```python
import strata

# Pack directory into snapshot
strata.build(
    "/data/imagenet",
    "imagenet.st",
    compression="zstd",
    cdc=True  # Enable deduplication
)
```

**CLI**:
```bash
strata data pack \\
  --disk /data/imagenet \\
  --output imagenet.st \\
  --compression zstd \\
  --cdc
```

### Stream from S3

```python
import strata

# Open snapshot from S3 (index downloads in seconds)
with strata.open("s3://bucket/dataset.st", s3_region="us-west-2") as reader:
    # Read sample at offset 1MB
    data = reader.read(4096, offset=1024*1024)
```

### Boot a VM

```bash
strata vm boot ubuntu.st \\
  --ram 4G \\
  --cpus 4 \\
  --net \\
  --forward 2222:22
```

---

## Performance

Representative benchmarks:

| Operation | Latency | Throughput |
|-----------|---------|------------|
| Random block access | 15µs | — |
| Sequential decompression (LZ4) | — | 2.1 GB/s |
| Sequential decompression (Zstd) | — | 890 MB/s |
| PyTorch DataLoader (8 workers) | — | 6400 img/s |
| S3 streaming (first epoch) | — | 120 MB/s |
| S3 streaming (cached) | — | 2.1 GB/s |

See [Benchmarks](project-docs/BENCHMARKS.md) for methodology and full results.

---

## Community & Support

- **GitHub**: [Alethic-Systems/strata](https://github.com/Alethic-Systems/strata)
- **Issues**: [Report bugs or request features](https://github.com/Alethic-Systems/strata/issues)
- **Discussions**: [Ask questions](https://github.com/Alethic-Systems/strata/discussions)
- **Contributing**: See [CONTRIBUTING.md](project-docs/CONTRIBUTING.md)

---

## License

Apache License 2.0 — See [LICENSE](https://github.com/Alethic-Systems/strata/blob/main/LICENSE)

---

## What to Read Next

**New to Strata?**
→ Start with [Getting Started Tutorial](tutorials/getting-started.md)

**ML Engineer?**
→ Read [Why Strata for ML](explanation/why-strata-for-ml.md), then [First ML Pipeline](tutorials/first-ml-pipeline.md)

**Systems Engineer?**
→ Try [Booting Your First VM](tutorials/booting-your-first-vm.md)

**Need help?**
→ Check [Troubleshooting](how-to/troubleshooting.md) or [open an issue](https://github.com/Alethic-Systems/strata/issues)
