//! Core archive engine: format, algorithms, cache, and read API.
//!
//! `hexz-core` is the minimal, dependency-light foundation for reading Hexz
//! archives. It has no network deps, no async runtime, and no write-path
//! parallelism. Those concerns live in `hexz-store` and `hexz-ops`.
//!
//! # Modules
//!
//! - **[`format`]**: On-disk binary structures (header, index, magic, version)
//! - **[`algo`]**: Compression, encryption, hashing, deduplication traits + impls
//! - **[`cache`]**: Sharded LRU cache for decompressed blocks and index pages
//! - **[`store`]**: [`StorageBackend`](store::StorageBackend) trait (implementations in `hexz-store`)
//! - **[`api`]**: [`Archive`] — the public read API

pub mod algo;
pub mod api;
pub mod cache;
pub mod format;
pub mod store;

pub use api::file::{Archive, ArchiveStream};
