//! PyO3 binding layer for Hexz snapshot I/O.
//!
//! This module provides the complete Python API for Hexz, enabling high-performance
//! snapshot reading, writing, and manipulation from Python code. All classes and functions
//! exposed to Python are defined here using PyO3 bindings.
//!
//! # Architecture
//!
//! The Python interface is organized into several specialized modules:
//!
//! - **[`reader`]**: Synchronous snapshot reader (`Reader`) implementing Python's
//!   file-like protocol with efficient cursor management and zero-copy buffer operations.
//!
//! - **[`async_reader`]**: Asynchronous snapshot reader (`AsyncReader`) with native
//!   Python `asyncio` integration via `pyo3-async-runtimes`. All I/O operations are executed
//!   on Tokio's blocking thread pool to avoid blocking the event loop.
//!
//! - **[`builder`]**: Low-level snapshot creation API (`Builder`) supporting model weight
//!   packing, overlay merging, compression, deduplication, and content-defined chunking.
//!
//! - **[`pack`]**: High-level packing function wrapping `core::ops::pack` for creating
//!   snapshots from Python without instantiating a builder manually.
//!
//! - **[`ops`]**: Utility functions for snapshot inspection, analysis, signing, verification,
//!   and live VM snapshotting via QMP.
//!
//! - **[`exceptions`]**: Custom Python exception types mapping Rust errors to a structured
//!   exception hierarchy (`Error`, `IOError`, `FormatError`, etc.).
//!
//! # Integration with PyO3
//!
//! This module uses PyO3 to expose Rust functionality to Python while maintaining safety
//! and performance:
//!
//! - **GIL Release**: Long-running operations (I/O, compression, hashing) release the Global
//!   Interpreter Lock via `py.allow_threads()`, enabling true parallelism when multiple
//!   threads call into Hexz.
//!
//! - **Buffer Protocol**: Direct integration with Python's buffer protocol allows zero-copy
//!   reads into NumPy arrays and other buffer-supporting types (like PyTorch tensors)
//!   via `read(buffer=...)` and `readinto()` methods.
//!
//! - **Context Managers**: All reader classes implement `__enter__`/`__exit__` (and async
//!   variants) for idiomatic Python resource management with `with` statements.
//!
//! - **Pickle Support**: Readers support pickling via `__getstate__`/`__setstate__` for
//!   multiprocessing and distributed training scenarios.
//!
//! # Usage Patterns
//!
//! ## Synchronous Reading (Checkpoint Access)
//!
//! ```python
//! from hexz import Reader
//! import torch
//!
//! # Open a model checkpoint and read specific weights
//! with Reader("llama-7b.hxz") as reader:
//!     # Read 1GB weight at known offset directly into a tensor buffer
//!     weights = torch.zeros(1024*1024*1024, dtype=torch.uint8)
//!     reader.read(buffer=weights.numpy(), offset=4096)
//!
//! # Random access without moving cursor
//! data = reader.read(1024, offset=8192)
//! ```
//!
//! ## Asynchronous Reading
//!
//! ```python
//! from hexz import AsyncReader
//! import asyncio
//!
//! async def load_weights():
//!     reader = await AsyncReader.create("model.hxz")
//!     data = await reader.read(4096, offset=0)
//!     return data
//!
//! asyncio.run(load_weights())
//! ```
//!
//! ## Snapshot Creation
//!
//! ```python
//! from hexz import Builder
//!
//! # Create snapshot with compression and deduplication
//! builder = Builder("output.hxz", compression="zstd", dedup=True)
//! builder.add_disk_file("weights.bin")
//! builder.finalize()
//! ```
//!
//! ## Overlay Merging (Fine-tuning)
//!
//! ```python
//! # Merge fine-tuned changes into thin snapshot (references base weights)
//! builder = Builder("finetuned-v1.hxz")
//! builder.merge_overlay(
//!     base_path="base_model.hxz",
//!     overlay_path="delta.bin",
//!     thin=True  # creates thin snapshot referencing base
//! )
//! builder.finalize()
//! ```
//!
//! # Performance Considerations
//!
//! - **Prefetching**: Configure `prefetch_count` when opening readers to enable background
//!   block prefetching for sequential access patterns.
//!
//! - **Cache Sizing**: Set `cache_capacity_bytes` based on working set size. Default is
//!   conservative; increase for better hit rates on random access.
//!
//! - **Buffer Reuse**: Use `read(buffer=...)` and `readinto()` to avoid allocations when
//!   reading into pre-allocated buffers (NumPy arrays, PyTorch tensors).
//!
//! - **Async Concurrency**: `AsyncReader` operations run on the Tokio blocking pool,
//!   allowing safe concurrent access from multiple coroutines without blocking the event loop.
//!
//! # Thread Safety
//!
//! - **Reader**: Thread-safe. Multiple threads can share the same reader instance
//!   (cursor is protected by a `Mutex`), but for best performance, use one reader per thread.
//!
//! - **AsyncReader**: Async-safe. Can be accessed from multiple coroutines, but I/O
//!   operations serialize internally via `spawn_blocking`.
//!
//! - **Builder**: NOT thread-safe. Use a single thread for building. The `finalize()`
//!   method consumes the builder, preventing accidental reuse.
//!
//! # Error Handling
//!
//! All errors are converted to Python exceptions via the [`exceptions`] module. Common
//! error types include:
//!
//! - `IOError`: File not found, permission denied, network failures
//! - `FormatError`: Invalid snapshot format, corrupted header
//! - `ValidationError`: Invalid parameters, unsupported compression
//! - `VersionError`: Incompatible format version
//!
//! See [`exceptions`] for the complete hierarchy and usage examples.

pub mod async_reader;
pub mod builder;
pub mod exceptions;
pub mod ops;
pub mod pack;
pub mod reader;
