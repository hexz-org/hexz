# Strata Architecture

## Overview

Strata is organized as a modular Rust workspace with clean separation between storage backends, compression algorithms, and user-facing interfaces.

```
┌─────────────────────────────────────────────────────────┐
│                    User Interfaces                       │
├──────────────┬──────────────┬──────────────┬────────────┤
│  Python API  │   CLI Tool   │  FUSE Mount  │  HTTP/NBD  │
│ (strata-loader) │  (strata-cli) │(strata-fuse) │(strata-server)│
└──────────────┴──────────────┴──────────────┴────────────┘
                        │
                        ▼
         ┌──────────────────────────────┐
         │      strata-core (Engine)     │
         │                               │
         │  ┌────────┐  ┌──────────┐    │
         │  │ format │  │   ops    │    │
         │  └────────┘  └──────────┘    │
         │  ┌────────┐  ┌──────────┐    │
         │  │ store  │  │   algo   │    │
         │  └────────┘  └──────────┘    │
         │  ┌────────┐  ┌──────────┐    │
         │  │  api   │  │  cache   │    │
         │  └────────┘  └──────────┘    │
         └──────────────────────────────┘
                        │
                        ▼
         ┌──────────────────────────────┐
         │   strata-common (Shared)     │
         │                               │
         │  • Error types                │
         │  • Configuration              │
         │  • Logging setup              │
         │  • Signing/crypto utils       │
         └──────────────────────────────┘
```

## Crate Organization

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
    └── lookup.rs    # Path resolution
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
├── ui/              # User interface helpers
│   └── progress.rs  # Progress bars (indicatif)
└── commands/        # Legacy command structure (to be removed)
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

2. **Zero-Copy Buffer Protocol**: The `readinto()` method uses CPython's buffer protocol to write directly into NumPy arrays without intermediate allocations.

3. **GIL Management**: All I/O operations use `py.allow_threads()` to release the GIL during blocking operations, enabling true parallelism in multi-worker DataLoaders.

## File Format

### On-Disk Layout

```
┌─────────────────────────────────────┐
│  Header (512 bytes)                 │
│  - Magic bytes: "STRATA\0\0"        │
│  - Version: u32                     │
│  - Block size: u32                  │
│  - Index offset: u64                │
│  - Compression type: u8             │
│  - Encryption: Option<...>          │
│  - Parent path: Option<String>      │
├─────────────────────────────────────┤
│  Compressed Data Blocks             │
│  (Variable size, LZ4/Zstd)          │
│  ...                                │
│  ...                                │
├─────────────────────────────────────┤
│  Page Indices (Variable size)       │
│  Each page: 64KB of BlockInfo[]     │
│  ...                                │
├─────────────────────────────────────┤
│  Master Index (at header.index_offset)│
│  - Disk size: u64                   │
│  - Memory size: u64                 │
│  - Disk page entries: Vec<PageEntry>│
│  - Memory page entries: Vec<...>    │
│  (Serialized with bincode)          │
└─────────────────────────────────────┘
```

**BlockInfo Structure**:
```rust
struct BlockInfo {
    offset: u64,      // Offset in file (0 = zero block, PARENT = parent ref)
    length: u32,      // Compressed size (0 if zero block)
    logical_len: u32, // Uncompressed size
    checksum: u32,    // CRC32 of compressed data
}
```

**Page Structure**:
```rust
struct IndexPage {
    blocks: Vec<BlockInfo>,  // Up to ENTRIES_PER_PAGE (1024)
}

struct PageEntry {
    offset: u64,        // Where this page is stored
    length: u32,        // Serialized size of page
    start_block: u64,   // First block number in this page
    start_logical: u64, // Logical offset of first block
}
```

### Read Path

```
User requests: read_at(offset=1MB, length=4KB)
    │
    ├─> StrataFile::read_at()
    │       │
    │       ├─> Find which blocks contain [1MB, 1MB+4KB)
    │       │       │
    │       │       └─> Binary search master index pages
    │       │
    │       ├─> Load relevant page(s) from disk
    │       │       │
    │       │       └─> Check LRU page cache first
    │       │
    │       ├─> For each block:
    │       │       │
    │       │       ├─> Check L1 block cache
    │       │       │
    │       │       ├─> If not cached:
    │       │       │   ├─> Read compressed data from backend
    │       │       │   ├─> Decrypt (if encrypted)
    │       │       │   ├─> Decompress (LZ4/Zstd)
    │       │       │   ├─> Verify checksum
    │       │       │   └─> Insert into L1 cache
    │       │       │
    │       │       └─> Extract relevant slice from block
    │       │
    │       └─> Concatenate slices into final buffer
    │
    └─> Return Vec<u8>
```

### Write Path (Packing)

```
User runs: strata data pack --disk image.img --output snap.st
    │
    ├─> Open input file(s)
    │
    ├─> Write placeholder header (512 bytes of zeros)
    │
    ├─> For each block_size chunk of input:
    │       │
    │       ├─> Check if all zeros → Skip write, record offset=0
    │       │
    │       ├─> Compress block (LZ4/Zstd)
    │       │
    │       ├─> If CDC enabled:
    │       │   ├─> Hash compressed block (BLAKE3)
    │       │   ├─> Check dedup map
    │       │   └─> If duplicate, reference existing offset
    │       │
    │       ├─> If encrypted:
    │       │   └─> Encrypt with AES-256-GCM (block_idx as nonce)
    │       │
    │       ├─> Compute CRC32 of (encrypted) compressed data
    │       │
    │       ├─> Write compressed block to file
    │       │
    │       └─> Record BlockInfo in current index page
    │       │
    │       └─> If page full (1024 blocks):
    │           ├─> Serialize page to bincode
    │           ├─> Write page to file
    │           ├─> Record PageEntry in master index
    │           └─> Start new page
    │
    ├─> Write final partial page (if any)
    │
    ├─> Serialize master index to bincode
    │
    ├─> Write master index at current offset
    │
    ├─> Seek back to beginning
    │
    ├─> Write header with index_offset pointing to master index
    │
    └─> Close file
```

## Performance Characteristics

### Compression Ratios

Measured on real-world datasets:

| Data Type        | LZ4 Ratio | Zstd Ratio | LZ4 Speed | Zstd Speed |
|------------------|-----------|------------|-----------|------------|
| OS Images        | 2.1x      | 3.4x       | 1850 MB/s | 420 MB/s   |
| Text (logs)      | 3.8x      | 7.2x       | 2100 MB/s | 380 MB/s   |
| Images (JPEG)    | 1.01x     | 1.02x      | 2800 MB/s | 950 MB/s   |
| Binary (random)  | 1.0x      | 1.0x       | 2900 MB/s | 1100 MB/s  |

**Takeaway**: Incompressible data (already compressed images, random data) still benefits from Strata's random access and streaming capabilities.

### Block Size Impact

Tested on 10GB OS image:

| Block Size | File Size | Random Read Latency | Sequential Throughput |
|------------|-----------|---------------------|-----------------------|
| 16 KB      | 5.2 GB    | 0.8 ms              | 1200 MB/s             |
| 64 KB      | 4.7 GB    | 1.1 ms              | 1650 MB/s             |
| 256 KB     | 4.1 GB    | 2.3 ms              | 1850 MB/s             |

**Recommendation**: 64KB for most use cases. Use 16KB for databases with small random I/O, 256KB for sequential-only access.

### Caching

**L1 Block Cache**:
- LRU eviction
- Default size: 128 MB (configurable)
- Stores decompressed blocks
- Hit rate on ML training: ~85% (with good locality)

**Page Index Cache**:
- LRU eviction
- Stores deserialized index pages
- Negligible memory (few KB per page)

**Memory Usage Estimate**:
```
Base: ~5 MB (StrataFile struct, metadata)
L1 Cache: ~128 MB (default)
Page Cache: ~1 MB per 1000 pages
Total: < 150 MB for typical use
```

## Security

### SSRF Prevention

HTTP/HTTPS backends validate URLs to prevent Server-Side Request Forgery:

```rust
// In store/utils.rs
fn is_restricted_ip(ip: IpAddr) -> bool {
    // Blocks:
    // - 127.0.0.0/8 (localhost)
    // - 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16 (private)
    // - 169.254.0.0/16 (link-local)
    // - 0.0.0.0/8 (unspecified)
}
```

Can be bypassed with `allow_restricted=true` for local development.

### Encryption

AES-256-GCM with:
- Key derived from password using Argon2
- Per-block nonces (block index as IV)
- Authenticated encryption (detects tampering)

### Signature Verification

Ed25519 signatures over snapshot index (future enhancement):
```
Signature covers: MasterIndex serialized bytes
Stored at: header.signature_offset
Verified on load: Optional (--verify flag)
```

## Future Enhancements

### Planned Features

1. **Streaming Packing**: Pack directly from stdin without temporary files
2. **Multi-Stream API**: Expose memory stream access in Python API
3. **Index Formats**: B-tree and hash-based indices for different access patterns
4. **Thin Snapshot Path Rewriting**: Update parent references when moving files
5. **Snapshot Merging**: Combine multiple snapshots into one
6. **Incremental Sync**: rsync-style protocol for network-efficient updates

### Experimental

- **WASM Builds**: Compile core to WebAssembly for browser access
- **GPU Decompression**: Offload decompression to GPU for ML training
- **Smart Prefetching**: ML-based prefetch prediction
- **Distributed Snapshots**: Shard large snapshots across multiple files/hosts
