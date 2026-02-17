//! High-level operations for Hexz snapshot files.
//!
//! This module provides the orchestration layer that combines low-level format,
//! storage, and algorithm primitives into complete end-to-end workflows for
//! creating, reading, and modifying Hexz archives.
//!
//! # Architecture
//!
//! The operations layer sits between command-line interfaces (CLI, Python) and
//! core primitives (format, store, algo):
//!
//! ```text
//! ┌─────────────────────────────────┐
//! │  Interfaces (CLI, Python, FUSE) │
//! └────────────┬────────────────────┘
//!              │
//! ┌────────────┴────────────┐
//! │  Operations (this mod)  │  High-level workflows
//! │  - pack: Create archives│
//! │  - write: Incremental   │
//! └────────────┬────────────┘
//!              │
//! ┌────────────┴─────────────────────────┐
//! │  Core Primitives                     │
//! │  - format: Headers, indices          │
//! │  - store: Backends (file, S3, HTTP)  │
//! │  - algo: Compression, encryption     │
//! └──────────────────────────────────────┘
//! ```
//!
//! # Available Operations
//!
//! ## Archive Creation
//!
//! - [`pack`]: Complete archive creation from disk/memory dumps
//!   - Chunking (fixed-size or CDC)
//!   - Compression (LZ4 or Zstandard)
//!   - Deduplication (BLAKE3 based)
//!   - Optional encryption (AES-256-GCM)
//!   - Dictionary training
//!
//! ## Incremental Writing
//!
//! - [`write`]: Write operations for overlay commits
//!   - Merge overlay deltas with base snapshot
//!   - Support for thin snapshots (reference parent)
//!   - Efficient delta encoding
//!
//! # Design Principles
//!
//! 1. **Interface Independence**: Operations are pure Rust functions, not CLI-specific
//! 2. **Composability**: Operations can be chained and reused across interfaces
//! 3. **Progress Reporting**: All long-running operations support progress callbacks
//! 4. **Error Handling**: Consistent `Result<T>` returns with descriptive errors
//!
//! # Usage from Python
//!
//! The operations in this module are exposed to Python via the `hexz_loader` extension:
//!
//! ```python
//! from hexz import pack
//!
//! # Create archive from Python
//! pack(
//!     disk="/path/to/disk.img",
//!     output="snapshot.hxz",
//!     compression="lz4",
//!     encrypt=False
//! )
//! ```

pub mod inspect;
pub mod pack;
pub mod parallel_pack;
pub mod progress;
#[cfg(feature = "signing")]
pub mod sign;
pub mod snapshot_writer;
pub mod write;
