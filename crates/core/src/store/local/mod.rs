/// File-backed storage backend using `pread` for thread-safe reads.
pub mod file;

/// Memory-mapped file backend leveraging the OS page cache.
pub mod mmap;

pub use file::FileBackend;
pub use mmap::MmapBackend;
