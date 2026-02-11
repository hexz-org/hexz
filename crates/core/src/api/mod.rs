//! Public API for snapshot file access.
//!
//! This module provides the high-level API for reading Strata snapshot archives.
//! It re-exports the primary types used by applications and libraries to interact
//! with `.st` files.
//!
//! # Primary Types
//!
//! - [`StrataFile`](stratafile::StrataFile): Main handle for reading snapshots
//! - [`SnapshotStream`](stratafile::SnapshotStream): Logical stream identifier (Disk or Memory)
//!
//! # Usage Example
//!
//! ```no_run
//! use strata_core::api::stratafile::{StrataFile, SnapshotStream};
//! use strata_core::store::local::FileBackend;
//! use strata_core::algo::compression::lz4::Lz4Compressor;
//! use std::sync::Arc;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Open a snapshot
//! let backend = Arc::new(FileBackend::new("vm.st".as_ref())?);
//! let compressor = Box::new(Lz4Compressor::new());
//! let snap = StrataFile::new(backend, compressor, None)?;
//!
//! // Read from disk stream
//! let data = snap.read_at(SnapshotStream::Disk, 0, 512)?;
//! println!("First sector: {:?}", &data[..64]);
//! # Ok(())
//! # }
//! ```
//!
//! # Design Philosophy
//!
//! The API is designed to be:
//! - **Simple**: Most operations need only `StrataFile` and `SnapshotStream`
//! - **Flexible**: Pluggable backends, compressors, and encryptors
//! - **Safe**: All operations return `Result<T>` with clear error types
//! - **Efficient**: Minimal copies, buffer reuse, parallel decompression

/// High-level snapshot file API.
///
/// Exposes `StrataFile` and related types that present logical disk and memory
/// streams backed by the on-disk snapshot format.
pub mod stratafile;
