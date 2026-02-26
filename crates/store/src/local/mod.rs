//! Local filesystem storage backends.
//!
//! - [`FileBackend`]: Traditional `pread(2)`-based I/O. Thread-safe, zero locks.
//! - [`MmapBackend`]: Memory-mapped zero-copy I/O. Default for local reads.

pub mod file;
pub mod mmap;

pub use file::FileBackend;
pub use mmap::MmapBackend;
