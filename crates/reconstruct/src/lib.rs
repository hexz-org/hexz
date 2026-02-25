//! Memory layout and I/O reconstruction research for the paper.
//!
//! This crate contains:
//! - [`store`]: Storage backends, starting with the mmap baseline
//! - `layout`: Memory layout strategies (bin-packed, log, slab)
//! - `io`: I/O batching via io_uring
//! - `engine`: Reconstruction orchestration
//! - `stats`: Measurement utilities

pub mod store;
