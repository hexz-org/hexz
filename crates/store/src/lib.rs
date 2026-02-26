//! Storage backend implementations for Hexz snapshots.
//!
//! This crate provides concrete implementations of `hexz_core::store::StorageBackend`
//! for local files, HTTP/HTTPS, and S3-compatible object storage. It also exposes a
//! `ParentLoader` helper for opening thin-snapshot parent chains via `FileBackend`.

pub mod http;
pub mod local;
pub mod runtime;
#[cfg(feature = "s3")]
pub mod s3;
pub mod utils;

pub use hexz_core::store::StorageBackend;

use hexz_common::Result;
use hexz_core::algo::compression::create_compressor;
use hexz_core::algo::encryption::Encryptor;
use hexz_core::api::file::{File, ParentLoader};
use hexz_core::format::header::Header;
use std::sync::Arc;

/// Opens a Hexz snapshot from a local file path, resolving any parent chain via
/// `FileBackend`. This is the standard entry point when working with local snapshots.
pub fn open_local(
    path: &std::path::Path,
    encryptor: Option<Box<dyn Encryptor>>,
) -> Result<Arc<File>> {
    open_local_with_cache(path, encryptor, None, None)
}

/// Like [`open_local`] but with custom cache and prefetch settings.
/// Like [`open_local`] but with custom cache and prefetch settings.
pub fn open_local_with_cache(
    path: &std::path::Path,
    encryptor: Option<Box<dyn Encryptor>>,
    cache_capacity_bytes: Option<usize>,
    prefetch_window_size: Option<u32>,
) -> Result<Arc<File>> {
    let backend: Arc<dyn StorageBackend> = Arc::new(local::MmapBackend::new(path)?);
    let header = Header::read_from_backend(backend.as_ref())?;
    let dictionary = header.load_dictionary(backend.as_ref())?;
    let compressor = create_compressor(header.compression, None, dictionary);

    let loader: ParentLoader = Box::new(|parent_path: &str| {
        let pb = std::path::Path::new(parent_path);
        let pb: Arc<dyn StorageBackend> = Arc::new(local::MmapBackend::new(pb)?);
        File::open(pb, None)
    });

    File::with_cache_and_loader(
        backend,
        compressor,
        encryptor,
        cache_capacity_bytes,
        prefetch_window_size,
        Some(&loader),
    )
}
