//! On-disk format structures for Hexz archive files.
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
//! ║   - Stream sizes (main, auxiliary)                       ║
//! ║   - Deduplication statistics                             ║
//! ╠══════════════════════════════════════════════════════════╣
//! ║ Optional: SIGNATURE (Ed25519, 64 bytes)                 ║
//! ╚══════════════════════════════════════════════════════════╝
//! ```
//!
//! # Submodules
//!
//! - `magic`: Magic bytes and version constants
//! - `header`: File header structure and enums
//! - `index`: Index pages and block metadata
//! - `version`: Version compatibility checking

pub mod magic;
pub mod header;
pub mod index;
pub mod version;
