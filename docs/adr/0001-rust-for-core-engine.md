# 1. Use Rust for Core Engine

Date: Early development phase

## Status

Accepted

## Context

Hexz requires high-performance, seekable access to compressed data with minimal latency. The system must handle:

- Random access to compressed blocks with low latency
- Zero-copy I/O operations for large datasets
- Concurrent access from multiple threads without lock contention
- Memory safety guarantees for production workloads
- Integration with Python ecosystems (PyTorch, NumPy)
- FUSE filesystem implementation for VM scenarios

Language alternatives considered:

- **C/C++**: Maximum performance but lacks memory safety
- **Go**: Excellent concurrency but garbage collection pauses
- **Python**: Ideal for ML integration but insufficient performance for core engine
- **Rust**: Memory safety without garbage collection, zero-cost abstractions, excellent FFI

The fundamental constraint is achieving low-latency block access while maintaining memory safety for production deployments.

## Decision

We will implement the core Hexz engine (file format, compression, deduplication, storage backends) in Rust.

The architecture will be:
- **Core Engine** (`hexz-core`): Pure Rust, no language bindings
- **CLI Tool** (`hexz-cli`): Rust binary for system administrators
- **Python Bindings** (`hexz-loader`): PyO3 wrapper exposing engine to Python
- **FUSE Interface** (`hexz-fuse`): Rust using `fuser` crate

This allows language-appropriate interfaces (Python for ML, CLI for ops) while keeping performance-critical code in Rust.

## Consequences

### Positive

- **Memory Safety**: Rust's ownership system eliminates entire classes of bugs (use-after-free, data races, buffer overflows) without runtime overhead
- **Predictable Performance**: No garbage collection pauses, consistent latency
- **Fearless Concurrency**: Rust's type system prevents data races at compile time
- **FFI Integration**: Strong C FFI and PyO3 support for Python bindings
- **Ecosystem Maturity**: Quality crates for compression (lz4, zstd), cryptography (ring, blake3), async I/O (tokio)
- **Cross-Platform**: Native support for Linux, macOS, Windows

### Negative

- **Steeper Learning Curve**: Contributors must learn Rust's ownership model and borrow checker
- **Longer Compile Times**: Rust compilation slower than interpreted languages
- **Smaller Talent Pool**: Fewer Rust developers than Python/Go in ML community
- **Build Complexity**: Requires Cargo + Maturin for Python builds

### Neutral

- **Build System**: Uses Cargo workspace with Makefile for convenience
- **Testing**: Requires both Rust unit tests and Python integration tests
- **Documentation**: Must maintain rustdoc for Rust API and Sphinx for Python API
- **Deployment**: Binary wheels for Python require CI/CD for multiple platforms
