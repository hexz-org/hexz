// unsafe required for: mmap-based file I/O and pread(2) syscalls.
// All unsafe blocks have individual SAFETY comments.
#![allow(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, unused_results))]

//! Storage backend implementations for Hexz archives.
//!
//! This crate provides concrete implementations of `hexz_core::store::StorageBackend`
//! for local files, HTTP/HTTPS, and S3-compatible object storage. It also exposes a
//! `ParentLoader` helper for opening thin-archive parent chains via `ArchiveBackend`.

pub mod http;
pub mod local;
pub mod remote;
pub mod runtime;
#[cfg(feature = "s3")]
pub mod s3;
pub mod utils;

pub use hexz_core::store::StorageBackend;

use hexz_common::Result;
use hexz_core::algo::compression::create_compressor;
use hexz_core::algo::encryption::Encryptor;
use hexz_core::api::file::{Archive, ParentLoader};
use hexz_core::format::header::Header;
use std::sync::Arc;

/// Opens a Hexz archive from a local file path, resolving any parent chain via
/// `ArchiveBackend`. This is the standard entry point when working with local archives.
pub fn open_local(
    path: &std::path::Path,
    encryptor: Option<Box<dyn Encryptor>>,
) -> Result<Arc<Archive>> {
    open_local_with_cache(path, encryptor, None, None)
}

/// Like [`open_local`] but with custom cache and prefetch settings.
/// Like [`open_local`] but with custom cache and prefetch settings.
pub fn open_local_with_cache(
    path: &std::path::Path,
    encryptor: Option<Box<dyn Encryptor>>,
    cache_capacity_bytes: Option<usize>,
    prefetch_window_size: Option<u32>,
) -> Result<Arc<Archive>> {
    let backend: Arc<dyn StorageBackend> = Arc::new(local::MmapBackend::new(path)?);
    let header = Header::read_from_backend(backend.as_ref())?;
    let dictionary = header.load_dictionary(backend.as_ref())?;
    let compressor = create_compressor(header.compression, None, dictionary.as_deref());

    let archive_dir = path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();
    let loader: ParentLoader = Box::new(move |parent_path: &str| {
        let p = std::path::Path::new(parent_path);
        let full_parent_path = if p.exists() {
            p.to_path_buf()
        } else {
            let rel = archive_dir.join(parent_path);
            if rel.exists() { rel } else { p.to_path_buf() }
        };
        let pb: Arc<dyn StorageBackend> = Arc::new(local::MmapBackend::new(&full_parent_path)?);
        Archive::open(pb, None)
    });

    Archive::with_cache_and_loader(
        backend,
        compressor,
        encryptor,
        cache_capacity_bytes,
        prefetch_window_size,
        Some(&loader),
    )
}
