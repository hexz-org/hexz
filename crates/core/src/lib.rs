//! Core snapshot engine providing block-level compressed storage with random access.
//!
//! # Overview
//!
//! `hexz-core` implements the core logic for creating and reading Hexz snapshots—
//! compressed, block-indexed archives that support random access, remote streaming,
//! and incremental updates. This crate contains no UI code; all user interfaces
//! (CLI, Python, FUSE) are in separate crates.
//!
//! # Architecture
//!
//! The crate is organized into several independent modules:
//!
//! - **[`mod@format`]**: On-disk structures (headers, indices) defining the file format
//! - **[`store`]**: Storage backend abstraction (local files, HTTP, S3)
//! - **[`algo`]**: Compression, encryption, hashing, and deduplication algorithms
//! - **[`cache`]**: LRU caching for decompressed blocks and index pages
//! - **[`api`]**: Public API ([`File`]) for reading snapshots
//! - **[`ops`]**: High-level operations for packing and manipulating snapshots
//!
//! # Quick Start
//!
//! ```no_run
//! use hexz_core::{File, SnapshotStream};
//! use hexz_core::store::local::FileBackend;
//! use hexz_core::algo::compression::lz4::Lz4Compressor;
//! use std::sync::Arc;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Open a local snapshot file
//! let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
//! let compressor = Box::new(Lz4Compressor::new());
//! let snapshot = File::new(backend, compressor, None)?;
//!
//! // Read 4KB from primary stream at offset 1MB
//! let data = snapshot.read_at(SnapshotStream::Primary, 1024 * 1024, 4096)?;
//! assert_eq!(data.len(), 4096);
//! # Ok(())
//! # }
//! ```
//!
//! # File Format
//!
//! Hexz snapshots consist of:
//! 1. A fixed-size header (512 bytes) with metadata
//! 2. Compressed data blocks (variable size)
//! 3. Hierarchical index pages (serialized with bincode)
//! 4. Master index at the end (location stored in header)
//!
//! The format supports:
//! - Block-level compression (LZ4, Zstandard)
//! - Optional AES-256-GCM encryption
//! - Thin snapshots (parent references)
//! - Dual streams (separate disk and memory data)
//! - Content-defined chunking for deduplication
//!
//! See [`mod@format`] module for detailed specification.
//!
//! # Storage Backends
//!
//! Storage backends implement the [`store::StorageBackend`] trait, enabling reads from:
//! - Local files ([`store::local::FileBackend`])
//! - Memory-mapped files ([`store::local::MmapBackend`])
//! - HTTP/HTTPS URLs ([`store::http`])
//! - S3 buckets ([`store::s3`])
//!
//! All backends provide the same interface—higher layers don't know where data comes from.
//!
//! # Compression & Encryption
//!
//! Compression and encryption are pluggable via traits:
//! - [`algo::compression::Compressor`]: LZ4 ([`algo::compression::lz4`]) or Zstandard ([`algo::compression::zstd`])
//! - [`algo::encryption::Encryptor`][]: AES-256-GCM ([`algo::encryption::aes_gcm`])
//!
//! Each block is compressed independently, then optionally encrypted. This enables:
//! - Parallel decompression (each block is self-contained)
//! - Random access (only decompress blocks you need)
//! - Block-level integrity (CRC32 checksums)
//!
//! # Performance
//!
//! - **Compression**: LZ4 ~2GB/s, Zstd ~500MB/s (single-threaded)
//! - **Random Access**: ~1ms latency (cold cache), ~0.08ms (warm cache)
//! - **Sequential Read**: ~2-3GB/s (NVMe storage, LZ4 decompression)
//! - **Memory**: <150MB typical (configurable block cache)
//!
//! # Thread Safety
//!
//! [`File`] is `Send + Sync` and can be safely shared across threads via `Arc`.
//! Internal caches use `Mutex` for synchronization. Multiple threads can read
//! concurrently from the same snapshot with independent cache hits.
//!
//! # Examples
//!
//! ## Reading from HTTP
//!
//! ```no_run
//! use hexz_core::File;
//! use hexz_core::store::http::HttpBackend;
//! use hexz_core::algo::compression::lz4::Lz4Compressor;
//! use std::sync::Arc;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let backend = Arc::new(HttpBackend::new(
//!     "https://example.com/dataset.hxz".to_string(),
//!     false // don't allow restricted IPs
//! )?);
//! let compressor = Box::new(Lz4Compressor::new());
//! let snapshot = File::new(backend, compressor, None)?;
//!
//! // Stream data without downloading entire file
//! let data = snapshot.read_at(hexz_core::SnapshotStream::Primary, 0, 1024)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Thin Snapshots (Parent References)
//!
//! Thin snapshots store a path to a base snapshot in their header; opening the
//! thin file automatically loads the parent when needed.
//!
//! ```no_run
//! use hexz_core::File;
//! use hexz_core::store::local::FileBackend;
//! use hexz_core::algo::compression::lz4::Lz4Compressor;
//! use std::sync::Arc;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Open thin snapshot (parent is loaded from header parent_path when present)
//! let thin_backend = Arc::new(FileBackend::new("incremental.hxz".as_ref())?);
//! let thin_compressor = Box::new(Lz4Compressor::new());
//! let thin = File::new(thin_backend, thin_compressor, None)?;
//!
//! // Reading from thin automatically falls back to base for unchanged blocks
//! let data = thin.read_at(hexz_core::SnapshotStream::Primary, 0, 4096)?;
//! # Ok(())
//! # }
//! ```

/// Public API surface for reading snapshot files.
///
/// Contains the main entry point for opening and reading snapshots.
///
/// See [`api::file`] module for the main types.
pub mod api;

/// Storage backend abstraction and implementations.
///
/// All backends implement [`store::StorageBackend`] to provide
/// uniform access to snapshot data regardless of source (local file, HTTP, S3).
pub mod store;

/// In-memory caching for decompressed blocks and deserialized index pages.
///
/// Caching is critical for performance—decompression is expensive. The LRU cache
/// implementation stores recently accessed blocks to avoid repeated decompression.
pub mod cache;

/// On-disk format structures: headers, indices, and serialization.
///
/// These types define the binary wire format for Hexz snapshots. All structures
/// use `bincode` for serialization and are versioned for forward compatibility.
///
/// See submodules for detailed format specification:
/// - `magic`: Magic bytes and version constants
/// - `header`: File header structure and enums
/// - `index`: Index pages and block metadata
/// - `version`: Version compatibility checking
pub mod format;

/// Algorithms for compression, encryption, hashing, and deduplication.
///
/// Each algorithm category has a trait definition and one or more implementations:
///
/// ## Compression
/// - LZ4: Fast compression
/// - Zstandard: High ratio compression
///
/// ## Encryption
/// - AES-256-GCM: Authenticated encryption
///
/// ## Hashing
/// - BLAKE3: Content-defined chunking
///
/// ## Deduplication
/// - FastCDC, DCAM modeling
pub mod algo;

/// High-level operations for creating, modifying, and analyzing snapshots.
///
/// Contains complex logic for creating and manipulating snapshots.
///
/// See submodules for specific operations.
pub mod ops;

pub use api::file::{File, SnapshotStream};
