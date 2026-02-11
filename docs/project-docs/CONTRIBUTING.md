# Contributing to Strata

Welcome to the Strata project.

Strata is a high-performance system designed for two distinct use cases:
1. AI Infrastructure: A streaming data loader for PyTorch.
2. Systems Virtualization: A seekable FUSE filesystem for VM booting.

Please read this guide to understand the development workflow and project structure.

---

## Git Workflow & Branching

We use a Stable Main workflow.

### The Branches
1. main: Production-ready code. Do not push directly to this branch. It is updated only via Pull Requests from dev.
2. dev: The integration branch. All feature branches merge here first.
3. feature/<name>: Your working branch for new tasks (e.g., feature/async-s3).

### The Workflow
1. Create a branch: git checkout -b feature/my-feature
2. Write your code.
3. Run tests.
4. Open a Pull Request targeting the dev branch.
5. Merge into dev once CI passes.

---

## Development Setup

All setup, build, and test commands go through the **Makefile** at the repo root. There are no separate setup scripts or one-off commands to remember.

### Prerequisites (checked by `make setup`)

Run **`make setup-check`** from the repo root. It will list any missing system packages and show install commands for your OS. You need:

* **Rust** — rustup + cargo (https://rustup.rs)
* **pkg-config** — for C library detection
* **Python 3** — for the AI loader and docs
* **libfuse** — dev headers (Linux: libfuse-dev or fuse2/fuse3; macOS: macFUSE via Homebrew)
* **qemu** — optional, only for VM boot tests

### Setup (one command)

1. Clone the repo and run the central setup from the repo root:
   ```bash
   git clone https://github.com/willmccallion/strata.git
   cd strata
   make setup
   ```
   This installs Rust components (rustfmt, clippy), cargo tools (cargo-deny, maturin, etc.), and creates a Python venv with docs requirements. If anything is missing, run **`make setup-check`** first and install what it suggests.

2. Verify the workspace and (optionally) install the Python loader for development:
   ```bash
   make check      # type-check
   make develop    # editable Python package for AI loader
   ```

---

## Project Architecture

Strata is a Rust Cargo Workspace.

| Directory | Crate Name | Description |
| :--- | :--- | :--- |
| crates/core | strata-core | File format, compression, and deduplication logic. Shared by all crates. |
| crates/loader | strata-loader | PyO3 bindings for the AI data loader. Contains S3 streaming logic. |
| crates/cli | strata-cli | The command-line tool for packing datasets and managing VMs. |
| crates/fuse | strata-fuse | FUSE adapter for mounting Strata filesystems (VM support). |
| crates/server | strata-server | HTTP server for streaming data. |

---

## Building & Running

### 1. Building the CLI Tool (Rust)

From repo root:

```bash
make rust
./target/release/strata --help
```

### 2. Building the Python Library (AI Loader)

From repo root (Makefile handles venv and maturin):

```bash
make develop
python -c "import strata; print(strata.__version__)"
```

---

## Testing

### Rust Unit Tests (Core Logic)

From repo root:

```bash
make test-rust
```

To run tests for a single crate only: `cargo test -p strata-core` (the Makefile does not define per-crate targets).

### Python Integration Tests (AI Loader)

From repo root:

```bash
make test-python
```

### VM Boot Tests (Systems)

Requires Linux and QEMU. From repo root:

```bash
./scripts/test_vm_boot.sh
```

---

## Style Guide

### Rust & Python

From repo root:

```bash
make fmt      # Format code
make lint     # Format check + clippy + ruff
```

---

## Pull Request Checklist

Before submitting to dev:
1. [x] Run full CI pipeline locally: `make ci`
2. [x] No large binary files or test artifacts committed
6. [x] Update relevant documentation if adding new features
7. [x] Pull Request title is descriptive and follows conventional commits style

---

## Getting Help

- Check the [documentation](../index.md) for technical documentation
- Look at existing examples in [examples/](https://github.com/willmccallion/strata/tree/main/examples)
- Open an issue if you find a bug or have questions
- Read the [ROADMAP.md](ROADMAP.md) to see planned features

---

## Code Organization Tips

### Adding New Features
1. **Core logic** goes in `crates/core/` (file format, compression, algorithms)
2. **Python bindings** go in `crates/loader/` (PyO3 wrappers)
3. **CLI commands** go in `crates/cli/src/commands/`
4. **Examples** should be self-contained in `examples/<name>/`

### Writing Tests
- Unit tests go in the same file as the code (Rust convention)
- Integration tests go in `crates/*/tests/`
- Test fixtures are in `crates/core/tests/common/`
- **Primary:** `make test` (Rust + Python). For a single test: `cargo test <test_name>`.

### Documentation
- Use `///` doc comments for public APIs
- **Primary:** `make docs` (rustdoc) and `make docs-python` (Sphinx). For a single crate: `cargo doc --open -p <crate>`.
- Examples in doc comments should be runnable (`cargo test --doc`)

## Detailed Crate Overview

### strata-common

**Purpose**: Shared types, errors, and utilities used across all crates.

**Key Modules**:
- `errors`: Unified error types with context
- `config`: Runtime configuration structures
- `logging`: Centralized tracing setup
- `sign`: Ed25519 signature generation/verification
- `constants`: Magic bytes, block offsets, defaults

**Design Philosophy**: Keep this crate minimal—only truly shared code belongs here. Crate-specific logic stays in respective crates.

### strata-core

**Purpose**: Core snapshot engine with no UI dependencies. All business logic for reading/writing snapshots lives here.

**Module Structure**:
```
core/
├── format/          # File format definitions
│   ├── header.rs    # Snapshot header structure
│   ├── magic.rs     # Magic bytes, version constants
│   └── index/       # Block index structures
│       ├── mod.rs   # MasterIndex, PageEntry
│       ├── btree.rs # B-tree index (future)
│       └── hash.rs  # Hash-based index (future)
├── store/           # Storage backend abstraction
│   ├── mod.rs       # StorageBackend trait
│   ├── local/       # Local file access
│   │   ├── file.rs  # Standard file I/O
│   │   └── mmap.rs  # Memory-mapped files
│   ├── http/        # HTTP(S) access
│   │   ├── sync.rs  # Synchronous client
│   │   └── async_client.rs # Async client
│   ├── s3/          # S3 access
│   │   ├── sync.rs  # Blocking S3 client
│   │   └── async_client.rs # Async S3 client
│   └── utils.rs     # URL validation, SSRF prevention
├── algo/            # Algorithms (compression, crypto, dedup)
│   ├── compression/ # Compression codecs
│   │   ├── mod.rs   # Compressor trait
│   │   ├── lz4.rs   # LZ4 implementation
│   │   └── zstd.rs  # Zstandard implementation
│   ├── encryption/  # Encryption
│   │   ├── mod.rs   # Encryptor trait
│   │   └── aes_gcm.rs # AES-GCM implementation
│   ├── hashing/     # Content hashing
│   │   ├── mod.rs   # ContentHasher trait
│   │   └── blake3.rs # BLAKE3 hasher
│   └── dedup/       # Deduplication
│       ├── mod.rs   # Dedup traits
│       ├── cdc.rs   # Content-defined chunking (FastCDC)
│       └── dcam.rs  # DCAM modeling
├── ops/             # High-level operations
│   ├── mod.rs       # Re-exports
│   ├── read.rs      # Read helpers (currently in api/)
│   ├── write.rs     # Write helpers
│   └── pack.rs      # Snapshot packing logic
├── cache/           # Caching layer
│   ├── mod.rs       # Cache traits
│   ├── lru.rs       # LRU block cache
│   ├── prefetch.rs  # Prefetch logic
│   └── policy.rs    # Eviction policies
└── api/             # Public API surface
    └── stratafile.rs # StrataFile (main entry point)
```

**Key Design Decisions**:

1. **Storage Backend Abstraction**: The `StorageBackend` trait allows reads from local files, HTTP, or S3 without changing upper layers. Each backend handles its own auth, retries, and error mapping.

2. **Compression/Encryption Separation**: Algorithms are trait-based, making it easy to add new codecs. The format layer doesn't know implementation details—only that it can call `compress()` or `decrypt()`.

3. **Lazy Index Loading**: The master index is only loaded on first access. Page indices are loaded on-demand and cached in an LRU.

4. **Block-Level Deduplication**: CDC (content-defined chunking) finds variable-sized chunks based on content, not fixed offsets. This enables deduplication across snapshots and incremental updates.

### strata-fuse

**Purpose**: FUSE filesystem interface for mounting snapshots.

**Module Structure**:
```
fuse/
├── vfs/             # Virtual filesystem layer
│   ├── mod.rs       # VFS core logic
│   ├── inode.rs     # Inode table and allocation
│   ├── attr.rs      # File attributes (stat, mode)
│   └── overlay.rs   # Copy-on-write overlay tracking
└── fuse/            # FUSE operations
    ├── mod.rs       # Filesystem trait impl
    ├── read.rs      # Read operations
    ├── write.rs     # Write operations (via overlay)
    ├── lookup.rs    # Path resolution
```

**Overlay Mechanism**:
- Mounts are read-only by default
- With `--overlay`, writes go to a separate file
- `.meta` file tracks which 4KB blocks have been modified
- On unmount, overlay + metadata can be committed back to a new snapshot

**Inode Management**:
- Flat inode space (directories are not deeply nested)
- Special inodes: `1` = disk, `2` = memory, `3` = metadata
- Supports `readdir`, `getattr`, `read`, `write` (via overlay)

### strata-cli

**Purpose**: Command-line interface for all operations.

**Module Structure**:
```
cli/
├── main.rs          # Clap setup, dispatch
├── args.rs          # Argument definitions
├── cmd/             # Command handlers (new structure)
│   ├── data/        # Data/snapshot commands
│   │   ├── pack.rs
│   │   ├── info.rs
│   │   └── diff.rs
│   ├── vm/          # VM commands
│   │   ├── boot.rs
│   │   ├── install.rs
│   │   ├── snap.rs
│   │   ├── commit.rs
│   │   └── mount.rs
│   └── sys/         # System utilities
│       ├── doctor.rs
│       ├── bench.rs
│       ├── serve.rs
│       └── keygen.rs
└── ui/              # User interface helpers
    └── progress.rs  # Progress bars (indicatif)
```

**Command Flow**:
```
User runs: strata data pack --disk x.img --output y.st
    │
    ├─> main.rs parses args via Clap
    │
    ├─> Dispatches to cmd::data::pack::run()
    │
    ├─> pack::run() calls core::ops::pack::pack_snapshot()
    │
    ├─> core creates StrataWriter, compresses blocks, writes index
    │
    └─> CLI shows progress bar, exits with result
```

### strata-loader (Python)

**Purpose**: Python bindings for ML/AI workflows.

**Module Structure**:
```
loader/
├── src/
│   ├── lib.rs           # PyO3 module entry point
│   ├── engine/          # Pure Rust (no PyO3)
│   │   ├── mod.rs       # open_snapshot(), read helpers
│   │   ├── iterator.rs  # Sequential block iteration
│   │   └── shuffle.rs   # Index shuffling (Fisher-Yates)
│   ├── py_interface/    # PyO3 bindings
│   │   ├── mod.rs
│   │   ├── dataset.rs   # StrataReader class
│   │   ├── async_dataset.rs # AsyncStrataReader
│   │   ├── builder.rs   # StrataBuilder (low-level)
│   │   ├── pack.rs      # pack() function
│   │   ├── ops.rs       # inspect, analyze, etc.
│   │   └── exceptions.rs # Error conversions
│   └── tensor/          # Zero-copy operations
│       ├── mod.rs
│       └── numpy.rs     # Buffer protocol FFI
└── python/
    └── strata/
        ├── __init__.py      # Package entry point
        ├── _strata_core.pyi # Type stubs
        ├── io.rs            # High-level wrappers
        ├── builder.py       # Pythonic builder API
        ├── mount.py         # Mount helper
        └── torch.py         # PyTorch integration
```

**Design Decisions**:

1. **Engine/Interface Separation**: Pure Rust logic in `engine/` has no PyO3 dependency, making it reusable for non-Python contexts (future C FFI, WASM, etc.).

2. **Zero-Copy Buffer Protocol**: The `read(buffer=...)` method uses CPython's buffer protocol to write directly into NumPy arrays without intermediate allocations.

3. **GIL Management**: All I/O operations use `py.allow_threads()` to release the GIL during blocking operations, enabling true parallelism in multi-worker DataLoaders.