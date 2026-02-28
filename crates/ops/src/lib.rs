//! High-level snapshot operations for Hexz.
//!
//! This crate contains the write path: packing, incremental writes, inspection,
//! and signing. It depends on `hexz-core` (format + algo + read API) and
//! `hexz-store` (I/O backends) but is not required for read-only consumers.

pub mod inspect;
pub mod pack;
pub mod parallel_pack;
pub mod parent_index;
pub mod predict;
pub mod progress;
pub mod safetensors;
#[cfg(feature = "signing")]
pub mod sign;
pub mod snapshot_writer;
pub mod write;
