# Strata Python API Reference

Browsable API documentation is generated with **Sphinx**. All build commands are run from the **repository root** via the **Makefile** (**`make help`** for the full list):

```bash
# Optional: install Sphinx (make docs-python uses it if present)
pip install sphinx

# From repo root
make develop
make docs-python
```

Then open `docs/_build/html/index.html` in a browser.

---

## Overview of public APIs

| API | Description |
|-----|-------------|
| **strata.open** | Open a snapshot for reading or writing (returns `Reader` or `Writer`) |
| **Reader** | File-like reader with `read()`, `read_at()`, `iter_chunks()`, slice notation |
| **AsyncReader** | Async context manager for reading snapshots |
| **Writer** | Build snapshots with `add()`, `add_file()`, `add_bytes()`, `add_array()` |
| **strata.build** | Build a snapshot from a path using a profile (`ml`, `generic`, `archival`, etc.) |
| **Dataset** | PyTorch `Dataset` backed by a Strata snapshot (fixed or variable-length items) |
| **TFDataset** | TensorFlow dataset wrapper (stub) |
| **read_array** / **write_array** | NumPy array I/O |
| **ArrayView** | Memmap-style view into array data in a snapshot |
| **strata.inspect** | Return `Metadata` for a snapshot |
| **strata.analyze** | Deduplication analysis → `AnalysisReport` |
| **strata.diff** | Compare two snapshots |
| **strata.verify** | Verify checksums, structure, and optionally signature |
| **strata.info** | Print human-readable snapshot info |
| **strata.mount** / **unmount** | Mount snapshot as FUSE filesystem |
| **strata.keygen** | Generate Ed25519 keypair for signing |
| **strata.sign_image** | Sign a snapshot with a private key |
| **strata.verify_image** | Verify a signed snapshot with a public key |
| **Metadata** | Snapshot metadata (version, compression, sizes, blocks) |
| **Exceptions** | `StrataError`, `IOError`, `FormatError`, `ValidationError`, etc. |

Each of these has docstrings and at least one code example in the Sphinx API reference (see **API Reference** in the built docs).

## Quick links

- [Quick start](../quickstart.md) — 5-minute tutorial
- [CLI usage](../usage/cli/README.md) — Packing and VM commands
- [AI loader usage](../usage/ai-loader/README.md) — ML streaming
