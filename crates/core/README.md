# hexz-core

Core engine for high-performance data streaming with compression and deduplication.

## Overview

`hexz-core` is the heart of the Hexz system—a seekable, deduplicated compression engine that enables random access to compressed data without decompressing entire archives. It provides block-level compression, content-defined chunking for deduplication, and pluggable storage backends for local files, HTTP, and S3.

This crate contains no UI code; all user interfaces (CLI, Python bindings, FUSE) are in separate crates.

## Architecture

```
hexz-core/
├── algo/         # Compression, dedup, encryption algorithms
│   ├── compression/    # LZ4, Zstandard
│   ├── encryption/     # AES-256-GCM
│   ├── dedup/          # FastCDC, content-defined chunking
│   └── hash/           # CRC32, BLAKE3
├── cache/        # LRU cache with prefetching
│   └── lru.rs          # Block and index page caching
├── format/       # File format handling
│   ├── header.rs       # Archive metadata (512 bytes)
│   ├── index.rs        # Hierarchical index structures
│   └── block.rs        # Compressed block format
├── store/        # Storage backends (local, HTTP, S3)
│   ├── local/          # FileBackend, MmapBackend
│   ├── http/           # Remote streaming over HTTP
│   └── s3/             # AWS S3/compatible object storage
├── api/          # Public API surface
│   └── file.rs   # Main entry point: File
└── ops/          # High-level operations
    └── pack/           # Create archives from raw data
```

## Quick Example

### Reading a Local Archive

```rust
use hexz_core::{File, ArchiveStream};
use hexz_core::store::local::FileBackend;
use hexz_core::algo::compression::lz4::Lz4Compressor;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open a local archive file
    let backend = Arc::new(FileBackend::new("archive.hxz".as_ref())?);
    let compressor = Box::new(Lz4Compressor::new());
    let archive = File::new(backend, compressor, None)?;

    // Read 4KB from main stream at offset 1MB
    let data = archive.read_at(ArchiveStream::Main, 1024 * 1024, 4096)?;
    assert_eq!(data.len(), 4096);

    Ok(())
}
```

### Streaming from HTTP

```rust
use hexz_core::File;
use hexz_core::store::http::HttpBackend;
use hexz_core::algo::compression::lz4::Lz4Compressor;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend = Arc::new(HttpBackend::new(
        "https://example.com/dataset.hxz".to_string(),
        false // don't allow restricted IPs
    )?);
    let compressor = Box::new(Lz4Compressor::new());
    let archive = File::new(backend, compressor, None)?;

    // Stream data without downloading entire file
    let data = archive.read_at(hexz_core::ArchiveStream::Main, 0, 1024)?;

    Ok(())
}
```

## Key Features

- **Random Access**: Read any byte range without decompressing the entire archive
- **Block-Level Compression**: LZ4 (~2GB/s) or Zstandard (~500MB/s) with independent blocks
- **Content-Defined Deduplication**: FastCDC chunking automatically eliminates duplicate blocks
- **Remote Streaming**: Stream from HTTP/S3 with intelligent block prefetching
- **Encryption**: Optional AES-256-GCM block-level encryption
- **Thin Archives**: Parent references for incremental backups
- **Thread-Safe**: `File` is `Send + Sync` with concurrent read support
- **Low Latency**: ~1ms cold cache, ~0.08ms warm cache random access
- **Pluggable Backends**: Uniform API for local files, memory-mapped files, HTTP, and S3

## File Format

Hexz archives consist of:

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

### Remote Streaming: When to Use It

Remote streaming (HTTP/S3) allows training without downloading datasets first. However, it's **not always the right choice**.

#### ✅ Remote Streaming Works Well When:

- **Dataset fits in cache** (most important!)
  - Example: 50 GB dataset, 64 GB cache → 95%+ hit rate after epoch 1
  - First epoch is slow (cold cache), subsequent epochs are fast
- **High bandwidth available** (10+ Gbps datacenter links)
- **Prototyping/experimentation** (one-off runs, convenience matters)
- **Storage is expensive** (cloud VMs with limited/costly local disk)

**Example performance:**
```
Dataset: 50 GB, Cache: 64 GB, Bandwidth: 10 Gbps
Epoch 1: ~2 hours (streaming from S3, cold cache)
Epoch 2-100: ~10 min each (95%+ cache hits, rarely hits network)
Total: ~18.5 hours
```

#### ❌ Remote Streaming Fails When:

- **Dataset >> cache size** (cache thrashing, slow every epoch)
  - Example: 2 TB dataset, 64 GB cache → <5% hit rate, constant streaming
  - Every epoch becomes a full network transfer (hours per epoch)
- **Limited bandwidth** (< 1 Gbps residential/cloud egress)
- **Repeated training** (download overhead amortizes over many runs)
- **Production workloads** (need predictable, fast performance)

**Example performance (BAD):**
```
Dataset: 2 TB, Cache: 64 GB (3% fits), Bandwidth: 1 Gbps
Every epoch: ~4.5 hours of data transfer
100 epochs: 450 hours = 18.75 days 😱
vs Download once (4.5h) + train (17h) = 21.5 hours total
```

#### Rule of Thumb

- **Dataset < cache size**: Remote streaming is viable
- **Dataset < 2x cache**: Marginal, depends on bandwidth and use case
- **Dataset > 2x cache**: Download to local NVMe/SSD first

#### Alternatives for Large Datasets

1. **Download to local storage** (fastest for repeated training)
   ```bash
   aws s3 cp s3://bucket/dataset.hxz /nvme/data/
   ```

2. **Subset/curriculum training** (stream only what you need)
   ```python
   # Train on 10% subsets, different each phase
   train(dataset_url, byte_range="0-100GB")
   ```

3. **Hierarchical caching** (future: RAM + SSD + remote)
   ```
   L1: 64 GB RAM (hot)
   L2: 1 TB SSD (warm)
   L3: S3 (cold)
   ```

Remote streaming is a **convenience feature**, not a replacement for local storage at TB scale. hexz's core value is **compression + dedup + fast random access**, with remote streaming as a nice-to-have for datasets that fit in cache.

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

`File` is `Send + Sync` and can be safely wrapped in `Arc` for multi-threaded access:

```rust
use std::sync::Arc;
use std::thread;

let archive = Arc::new(archive);

let handles: Vec<_> = (0..4)
    .map(|i| {
        let archive = Arc::clone(&archive);
        thread::spawn(move || {
            // Each thread can read independently with its own cache hits
            archive.read_at(ArchiveStream::Main, i * 4096, 4096)
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
# Build entire workspace (includes hexz-core)
make rust

# Build in debug mode for faster compilation
cargo build -p hexz-core

# Build with specific features
cargo build -p hexz-core --features s3,encryption
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
cargo test -p hexz-core
cargo test -p hexz-core --test integration
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
- **[API Documentation](https://docs.rs/hexz-core)** - Full API reference on docs.rs
- **[CLI Tool](../cli/)** - Command-line interface for creating archives
- **[Python Bindings](../loader/)** - PyTorch integration for ML workflows
- **[Project README](../../README.md)** - Main project overview
