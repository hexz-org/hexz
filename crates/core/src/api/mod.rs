//! Public API for snapshot file access.
//!
//! This module provides the high-level API for reading Hexz snapshot archives.
//! It re-exports the primary types used by applications and libraries to interact
//! with `.hxz` files.
//!
//! # Primary Types
//!
//! - `File`: Main handle for reading snapshots
//! - `SnapshotStream`: Logical stream identifier (Disk or Memory)
//!
//! See the `file` submodule for these types.
//!
//! # Usage Example
//!
//! ```no_run
//! use hexz_core::api::file::{File, SnapshotStream};
//! use hexz_core::store::local::FileBackend;
//! use hexz_core::algo::compression::lz4::Lz4Compressor;
//! use std::sync::Arc;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Open a snapshot
//! let backend = Arc::new(FileBackend::new("vm.hxz".as_ref())?);
//! let compressor = Box::new(Lz4Compressor::new());
//! let snap = File::new(backend, compressor, None)?;
//!
//! // Read from primary stream
//! let data = snap.read_at(SnapshotStream::Primary, 0, 512)?;
//! println!("First sector: {:?}", &data[..64]);
//! # Ok(())
//! # }
//! ```
//!
//! # Design Philosophy
//!
//! The API is designed to be:
//! - **Simple**: Most operations need only `File` and `SnapshotStream`
//! - **Flexible**: Pluggable backends, compressors, and encryptors
//! - **Safe**: All operations return `Result<T>` with clear error types
//! - **Efficient**: Minimal copies, buffer reuse, parallel decompression

/// High-level snapshot file API.
///
/// Exposes `File` and related types that present logical disk and memory
/// streams backed by the on-disk snapshot format.
pub mod file;
