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
//! - [`diff`]: Compare block hashes between two archives
//! - [`ls`]: List archives in a directory as a lineage tree
//! - [`analyze`]: Run DCAM analysis to optimize CDC parameters (diagnostics)
//! - [`overlay`]: Inspect FUSE overlay files (diagnostics)
//!
//! # Workflow Example
//!
//! ```bash
//! # Save two checkpoints then compare them
//! hexz diff base.hxz finetuned.hxz
//!
//! # List all checkpoints in a directory as a lineage tree
//! hexz ls ./checkpoints/
//!
//! # Inspect a single archive
//! hexz inspect snapshot.hxz --json
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

pub mod build;
pub mod convert;
pub mod diff;
pub mod inspect;
pub mod ls;
pub mod pack;

#[cfg(feature = "diagnostics")]
pub mod analyze;

#[cfg(feature = "diagnostics")]
pub mod overlay;
