//! Data operation commands for archive management.
//!
//! This module provides commands for creating, inspecting, and analyzing Hexz
//! archives. Archives (`.hxz` files) are the primary storage format for snapshots,
//! containing compressed, deduplicated, and optionally encrypted data.
//!
//! # Available Commands
//!
//! - [`pack`]: Create archives from raw disk images or memory dumps
//! - [`build`]: Build archives from source directories with profiles
//! - [`inspect`]: Inspect archive metadata (header, index, compression stats)
//! - [`diff`]: Show block/file-level differences in overlays (diagnostics)
//! - [`analyze`]: Run DCAM analysis to optimize CDC parameters (diagnostics)
//!
//! # Workflow Example
//!
//! ```bash
//! # 1. Analyze optimal parameters
//! hexz analyze disk.img
//!
//! # 2. Pack with optimized settings
//! hexz pack --disk disk.img --output snapshot.st --cdc \
//!   --min-chunk 8192 --avg-chunk 32768
//!
//! # 3. Inspect the result
//! hexz inspect snapshot.st --json
//! ```
//!
//! # Archive Format
//!
//! Archives consist of:
//! - **Header**: Magic bytes, version, flags, encryption metadata
//! - **Index**: B-tree or hash-based block index for fast lookups
//! - **Data**: Compressed, deduplicated blocks
//! - **Signature**: Optional Ed25519 signature (if signing enabled)
//!
//! # Performance Considerations
//!
//! - **CDC vs Fixed**: CDC provides better deduplication but slower packing
//! - **Compression**: LZ4 is faster, Zstandard has higher ratios
//! - **Dictionary Training**: Improves Zstandard compression by 10-30%
//! - **Block Size**: Larger blocks = less overhead, worse deduplication

pub mod inspect;
pub mod pack;

#[cfg(feature = "diagnostics")]
pub mod diff;

#[cfg(feature = "diagnostics")]
pub mod analyze;

pub mod build;

pub mod convert;
