//! On-disk format structures for Hexz snapshot files.
//!
//! This module defines the binary format of `.hxz` files, including headers,
//! indices, and metadata structures. All types are serialized with `bincode`
//! and must maintain backward compatibility across versions.
//!
//! # File Structure
//!
//! A complete Hexz archive has the following layout:
//!
//! ```text
//! ╔══════════════════════════════════════════════════════════╗
//! ║                  HEXZ ARCHIVE (.hxz)                    ║
//! ╠══════════════════════════════════════════════════════════╣
//! ║ Offset 0: HEADER (4096 bytes)                           ║
//! ║   - Magic: "HEXZ" (4 bytes)                             ║
//! ║   - Version: u32                                         ║
//! ║   - Block size: u32                                      ║
//! ║   - Index offset: u64                                    ║
//! ║   - Compression: enum (LZ4/Zstd)                         ║
//! ║   - Features: bitflags                                   ║
//! ║   - Optional: dictionary, metadata, signature offsets    ║
//! ╠══════════════════════════════════════════════════════════╣
//! ║ DATA REGION (variable size)                             ║
//! ║   - Compressed blocks                                    ║
//! ║   - Optional: encrypted blocks                           ║
//! ║   - Optional: compression dictionary                     ║
//! ╠══════════════════════════════════════════════════════════╣
//! ║ INDEX REGION (variable size)                            ║
//! ║   - Index pages (B-tree or hash-based)                   ║
//! ║   - Block metadata (offset, length, CRC32)               ║
//! ╠══════════════════════════════════════════════════════════╣
//! ║ MASTER INDEX (at header.index_offset)                   ║
//! ║   - Page entries                                         ║
//! ║   - Stream sizes (disk, memory)                          ║
//! ║   - Deduplication statistics                             ║
//! ╠══════════════════════════════════════════════════════════╣
//! ║ Optional: SIGNATURE (Ed25519, 64 bytes)                 ║
//! ╚══════════════════════════════════════════════════════════╝
//! ```
//!
//! # Format Versioning
//!
//! The format version follows semantic versioning:
//! - **Major**: Incompatible changes (readers must reject)
//! - **Minor**: Backward-compatible additions (old readers work)
//! - **Patch**: Bug fixes, no format changes
//!
//! Current version: See [`magic`] module for version constants
//!
//! # Serialization
//!
//! All structures use `bincode` with the following settings:
//! - **Endianness**: Little-endian
//! - **Size limits**: Bounded (prevents DOS attacks)
//! - **Compatibility**: Fixed-size where possible
//!
//! # Submodules
//!
//! - `magic`: Magic bytes and version constants
//! - `header`: File header structure and enums
//! - `index`: Index pages and block metadata
//! - `version`: Version compatibility checking

/// Magic bytes and version constants.
///
/// Defines the file signature (`HEXZ`) and format version that identify
/// a valid snapshot file.
///
/// **Critical Note:** The magic bytes **never change**—they permanently identify
/// this as a Hexz file across all versions. Altering them would make all
/// existing files unreadable.
///
/// The format version constant, however, **can and will change** to indicate
/// incompatible format updates. Readers must check this version and reject files
/// they cannot decode.
pub mod magic;

/// Snapshot on-disk header format.
///
/// Types in this module define the fixed-size header that precedes all other
/// data in a `.hxz` file and describe global format parameters.
pub mod header;

/// Snapshot index layout and block metadata structures.
///
/// This module contains the master index and per-page entries that map
/// logical blocks to their physical location in the file.
pub mod index;

/// Version compatibility checking.
///
/// Provides version range checking and graceful degradation for reading
/// files created with different format versions.
pub mod version;

/// Safetensors file format parser and writer.
///
/// Implements zero-copy reading of safetensors files for ingestion into
/// Hexz archives, and reconstruction of safetensors files from Hexz archives.
pub mod safetensors;
