//! Magic bytes and version constants for Strata snapshot files.
//!
//! This module defines the file signature and format version that identify
//! a valid `.st` file. These constants must remain stable across versions
//! to ensure backward compatibility.

/// The magic bytes identifying a Strata file.
///
/// Every `.st` file begins with this 4-byte signature: `STRT`.
pub const MAGIC_BYTES: &[u8; 4] = b"STRT";

/// The current format version.
///
/// This version number is incremented when the on-disk format changes
/// in a way that breaks backward compatibility.
pub const FORMAT_VERSION: u32 = 1;

/// The size of the file header in bytes.
///
/// The header is a fixed 4096-byte region at the start of every `.st` file
/// that contains metadata about compression, encryption, block size, etc.
pub const HEADER_SIZE: usize = 4096;
