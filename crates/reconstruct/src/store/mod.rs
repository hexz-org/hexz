//! Storage backends for reconstruction research.
//!
//! - [`MmapBackend`]: Naive mmap baseline (paper Section 3.1)
//!
//! Future modules: arena_mmap, slab_mmap, uring_io

pub mod mmap;

pub use mmap::MmapBackend;
