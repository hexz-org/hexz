//! Public API for archive file access.
//!
//! This module provides the high-level API for reading Hexz archives.
//! It re-exports the main types used by applications and libraries to interact
//! with `.hxz` files.
//!
//! # Main Types
//!
//! - `Archive`: Main handle for reading archives
//! - `ArchiveStream`: Logical stream identifier (Main or Auxiliary)
//! - `ArchiveManifest`: Directory tree manifest for file-based archives
//!
//! # Usage Example
//!
//! ```ignore
//! use hexz_core::api::file::{Archive, ArchiveStream};
//! use hexz_store::local::FileBackend;
//! use hexz_core::algo::compression::lz4::Lz4Compressor;
//! use std::sync::Arc;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Open an archive
//! let backend = Arc::new(FileBackend::new("data.hxz".as_ref())?);
//! let compressor = Box::new(Lz4Compressor::new());
//! let archive = Archive::new(backend, compressor, None)?;
//!
//! // Read from main stream
//! let data = archive.read_at(ArchiveStream::Main, 0, 512)?;
//! println!("First sector: {:?}", &data[..64]);
//! # Ok(())
//! # }
//! ```

/// High-level archive file API.
pub mod file;

/// Archive manifest for directory-based archives.
pub mod manifest;
