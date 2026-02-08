//! High-level operations for Strata snapshot files.
//!
//! This module provides the "business logic" layer that orchestrates
//! reading, writing, and packing operations. These operations combine
//! the lower-level format, store, and algo modules to perform complete
//! end-to-end workflows.
//!
//! The ops layer enables:
//! - Python bindings to call pack/read/write directly without CLI
//! - CLI commands to delegate to pure Rust functions
//! - Clear separation of I/O logic from command-line parsing

pub mod pack;
pub mod write;
