# strata-core

Core engine for high-performance data streaming with compression and deduplication.

## Overview

`strata-core` is the heart of the Strata system—a seekable, deduplicated compression engine that enables random access to compressed data without decompressing entire archives. It provides block-level compression, content-defined chunking for deduplication, and pluggable storage backends for local files, HTTP, and S3.

This crate contains no UI code; all user interfaces (CLI, Python bindings, FUSE) are in separate crates.

## Architecture

```
strata-core/
├── algo/         # Compression, dedup, encryption algorithms
│   ├── compression/    # LZ4, Zstandard
│   ├── encryption/     # AES-256-GCM
│   ├── dedup/          # FastCDC, content-defined chunking
│   └── hash/           # CRC32, BLAKE3
├── cache/        # LRU cache with prefetching
│   └── lru.rs          # Block and index page caching
├── format/       # File format handling
│   ├── header.rs       # Snapshot metadata (512 bytes)
│   ├── index.rs        # Hierarchical index structures
│   └── block.rs        # Compressed block format
├── store/        # Storage backends (local, HTTP, S3)
│   ├── local/          # FileBackend, MmapBackend
│   ├── http/           # Remote streaming over HTTP
│   └── s3/             # AWS S3/compatible object storage
├── api/          # Public API surface
│   └── stratafile.rs   # Main entry point: StrataFile
└── ops/          # High-level operations
    └── pack/           # Create snapshots from raw data
```

## Quick Example

### Reading a Local Snapshot

```rust
use strata_core::{StrataFile, SnapshotStream};
use strata_core::store::local::FileBackend;
use strata_core::algo::compression::lz4::Lz4Compressor;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open a local snapshot file
    let backend = Arc::new(FileBackend::new("snapshot.st".as_ref())?);
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None)?;

    // Read 4KB from disk stream at offset 1MB
    let data = snapshot.read_at(SnapshotStream::Disk, 1024 * 1024, 4096)?;
    assert_eq!(data.len(), 4096);

    Ok(())
}
```

### Streaming from HTTP

```rust
use strata_core::StrataFile;
use strata_core::store::http::HttpBackend;
use strata_core::algo::compression::lz4::Lz4Compressor;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend = Arc::new(HttpBackend::new(
        "https://example.com/dataset.st".to_string(),
        false // don't allow restricted IPs
    )?);
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None)?;

    // Stream data without downloading entire file
    let data = snapshot.read_at(strata_core::SnapshotStream::Disk, 0, 1024)?;

    Ok(())
}
```

## Key Features

- **Random Access**: Read any byte range without decompressing the entire archive
- **Block-Level Compression**: LZ4 (~2GB/s) or Zstandard (~500MB/s) with independent blocks
- **Content-Defined Deduplication**: FastCDC chunking automatically eliminates duplicate blocks
- **Remote Streaming**: Stream from HTTP/S3 with intelligent block prefetching
- **Encryption**: Optional AES-256-GCM block-level encryption
- **Thin Snapshots**: Parent references for incremental backups
- **Thread-Safe**: `StrataFile` is `Send + Sync` with concurrent read support
- **Low Latency**: ~1ms cold cache, ~0.08ms warm cache random access
- **Pluggable Backends**: Uniform API for local files, memory-mapped files, HTTP, and S3

## File Format

Strata snapshots consist of:

1. **Header** (512 bytes): Metadata, compression algorithm, encryption info, parent path
2. **Data Blocks**: Variable-size compressed blocks (typically 64KB-256KB)
3. **Index Pages**: Hierarchical B-tree-like index for fast lookups
4. **Master Index**: Points to root index page (location stored in header)

Each block is independently compressed and checksummed (CRC32), enabling:
- Parallel decompression
- Random access to individual blocks
- Block-level integrity verification

See the [file format specification](../../docs/reference/file-format-spec.md) for detailed specification.

## Performance Characteristics

| Metric | Value |
|--------|-------|
| Compression (LZ4) | ~2 GB/s |
| Compression (Zstd) | ~500 MB/s |
| Random Access (cold) | ~1 ms |
| Random Access (warm) | ~0.08 ms |
| Sequential Read | ~2-3 GB/s (NVMe + LZ4) |
| Memory Usage | <150 MB (configurable) |
| Deduplication | Up to 40% storage savings |

## Storage Backends

All backends implement the `StorageBackend` trait:

- **FileBackend**: Standard file I/O
- **MmapBackend**: Memory-mapped files (zero-copy reads)
- **HttpBackend**: Remote streaming via HTTP/HTTPS
- **S3Backend**: AWS S3 and S3-compatible object storage

Higher layers (API, cache, decompression) don't know where data comes from—all backends provide the same interface.

## Compression & Encryption

Pluggable algorithms via traits:

### Compression
- **LZ4**: Fast compression (~2GB/s), good for real-time workloads
- **Zstandard**: Better ratios (~500MB/s), configurable compression levels

### Encryption (optional)
- **AES-256-GCM**: Authenticated encryption with key derivation (PBKDF2)
- Each block encrypted independently
- Metadata encrypted separately

## Thread Safety

`StrataFile` is `Send + Sync` and can be safely wrapped in `Arc` for multi-threaded access:

```rust
use std::sync::Arc;
use std::thread;

let snapshot = Arc::new(snapshot);

let handles: Vec<_> = (0..4)
    .map(|i| {
        let snapshot = Arc::clone(&snapshot);
        thread::spawn(move || {
            // Each thread can read independently with its own cache hits
            snapshot.read_at(SnapshotStream::Disk, i * 4096, 4096)
        })
    })
    .collect();

for handle in handles {
    let data = handle.join().unwrap()?;
}
```

## Development

All development commands use the project Makefile. From the repository root:

### Building

```bash
# Build entire workspace (includes strata-core)
make rust

# Build in debug mode for faster compilation
cargo build -p strata-core

# Build with specific features
cargo build -p strata-core --features s3,encryption
```

### Testing

```bash
# Run all tests (Rust + Python)
make test

# Run only Rust tests
make test-rust

# Run tests with filter
make test-rust cache

# Or use cargo directly for this crate
cargo test -p strata-core
cargo test -p strata-core --test integration
```

### Linting & Formatting

```bash
# Format all code
make fmt

# Check formatting + clippy
make lint

# Run clippy with strict lints
make clippy
```

### Benchmarks

```bash
# Run all benchmarks
make bench

# Run specific benchmark
make bench cache

# Compare against archived baseline
make bench-compare baseline-v1
```

See `make help` for all available commands.

## Cargo Features

- `default`: `["compression-zstd", "encryption", "s3"]`
- `compression-zstd`: Zstandard compression support
- `encryption`: AES-256-GCM encryption
- `s3`: S3 storage backend

## See Also

- **[User Documentation](../../docs/)** - Tutorials, how-to guides, explanations
- **[API Documentation](https://docs.rs/strata-core)** - Full API reference on docs.rs
- **[CLI Tool](../cli/)** - Command-line interface for creating snapshots
- **[Python Bindings](../loader/)** - PyTorch integration for ML workflows
- **[Project README](../../README.md)** - Main project overview
