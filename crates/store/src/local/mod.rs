//! Local filesystem storage backends.
//!
//! - [`FileBackend`]: Traditional `pread(2)`-based I/O. Thread-safe, zero locks.
//! - [`MmapBackend`]: Memory-mapped I/O. Lives in `hexz-reconstruct` for paper work.

pub mod file;

pub use file::FileBackend;
