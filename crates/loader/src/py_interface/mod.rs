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
//! - **[`dataset`]**: Synchronous snapshot reader (`Reader`) implementing Python's
//!   file-like protocol with efficient cursor management and zero-copy buffer operations.
//!
//! - **[`async_dataset`]**: Asynchronous snapshot reader (`AsyncReader`) with native
//!   Python `asyncio` integration via `pyo3-async-runtimes`. All I/O operations are executed
//!   on Tokio's blocking thread pool to avoid blocking the event loop.
//!
//! - **[`builder`]**: Low-level snapshot creation API (`Builder`) supporting disk/memory
//!   image packing, overlay merging, compression, deduplication, and content-defined chunking.
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
//!   reads into NumPy arrays and other buffer-supporting types via `read_at_into()` and
//!   `readinto()` methods.
//!
//! - **Context Managers**: All reader classes implement `__enter__`/`__exit__` (and async
//!   variants) for idiomatic Python resource management with `with` statements.
//!
//! - **Pickle Support**: Readers support pickling via `__getstate__`/`__setstate__` for
//!   multiprocessing and distributed training scenarios.
//!
//! # Usage Patterns
//!
//! ## Synchronous Reading
//!
//! ```python
//! from hexz import Reader
//! import numpy as np
//!
//! # Open snapshot and read sequentially
//! reader = Reader("dataset.hxz")
//! chunk1 = reader.read(4096)  # reads from cursor
//! chunk2 = reader.read(4096)  # advances cursor
//!
//! # Random access without moving cursor
//! data = reader.read(1024, offset=8192)
//!
//! # Zero-copy into NumPy array
//! buffer = np.zeros(1024, dtype=np.uint8)
//! bytes_read = reader.read_at_into(offset=0, buffer=buffer)
//! ```
//!
//! ## Asynchronous Reading
//!
//! ```python
//! from hexz import AsyncReader
//! import asyncio
//!
//! async def process_snapshot():
//!     reader = await AsyncReader.create("dataset.hxz")
//!     data = await reader.read(4096)
//!     await reader.seek(0)
//!     return data
//!
//! asyncio.run(process_snapshot())
//! ```
//!
//! ## Snapshot Creation
//!
//! ```python
//! from hexz import Builder
//!
//! # Create snapshot with compression and deduplication
//! builder = Builder("output.hxz", compression="zstd", dedup=True)
//! builder.add_disk_file("disk.img")
//! builder.finalize()
//! ```
//!
//! ## Overlay Merging
//!
//! ```python
//! # Merge overlay changes into thin snapshot (references parent)
//! builder = Builder("merged.hxz")
//! builder.merge_overlay(
//!     base_path="base.hxz",
//!     overlay_path="overlay.img",
//!     thin=True  # creates thin snapshot referencing base
//! )
//! builder.finalize()
//! ```
//!
//! # Performance Considerations
//!
//! - **Prefetching**: Configure `prefetch_count` when opening readers to enable background
//!   block prefetching for sequential access patterns (ML training).
//!
//! - **Cache Sizing**: Set `cache_capacity_bytes` based on working set size. Default is
//!   conservative; increase for better hit rates on random access.
//!
//! - **Buffer Reuse**: Use `read_at_into()` and `readinto()` to avoid allocations when
//!   reading into pre-allocated buffers (NumPy arrays, ByteArrays).
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

pub mod async_dataset;
pub mod builder;
pub mod dataset;
pub mod exceptions;
pub mod ops;
pub mod pack;
