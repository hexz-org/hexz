//! On-disk format structures for Strata snapshot files.
//!
//! Defines the header (magic, version, block size, compression, features)
//! and the index layout (master index, pages, block metadata) that are
//! serialized with bincode and must remain backward compatible.

/// Magic bytes and version constants.
///
/// Defines the file signature (`STRT`) and format version that identify
/// a valid `.st` file.
pub mod magic;

/// Snapshot on-disk header format.
///
/// Types in this module define the fixed-size header that precedes all other
/// data in a `.st` file and describe global format parameters.
pub mod header;

/// Snapshot index layout and block metadata structures.
///
/// This module contains the master index and per-page entries that map
/// logical blocks to their physical location in the file.
pub mod index;
