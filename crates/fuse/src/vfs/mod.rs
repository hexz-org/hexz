//! Virtual filesystem abstractions for FUSE.
//!
//! This module implements the virtual filesystem layer that maps Strata snapshots
//! to POSIX-style inodes, attributes, and directory entries. It also manages the
//! overlay file for copy-on-write semantics.
//!
//! # Components
//!
//! - [`inode`]: Inode numbering, directory entries, inode metadata
//! - [`attr`]: File attribute construction (size, mode, timestamps)
//! - [`overlay`]: Copy-on-write overlay for writable mounts
//!
//! # Inode Model
//!
//! The VFS uses a **static inode table** with three fixed entries:
//!
//! ```text
//! Root Directory (inode 1)
//! ├── disk (inode 2) → StrataFile::read_at(Disk, ...)
//! └── memory (inode 3) → StrataFile::read_at(Memory, ...)
//! ```
//!
//! This design:
//! - Simplifies implementation (no dynamic inode allocation)
//! - Provides stable inode numbers across mounts
//! - Maps directly to snapshot streams
//!
//! # Overlay Format
//!
//! The overlay file stores:
//! 1. **Modified blocks** (4KB granularity)
//! 2. **Dirty block bitmap** (metadata)
//! 3. **Extended size** (if written beyond snapshot size)
//!
//! Format details: See [`overlay::Overlay`]
//!
//! # Permission Model
//!
//! - **Owner**: Configurable UID/GID (default: 1000:1000)
//! - **Mode**: 0644 for files, 0755 for root directory
//! - **ACLs**: Not supported (FUSE default permissions)

pub mod attr;
pub mod inode;
pub mod overlay;

pub use inode::{DirEntry, Inode, InodeMap, InodeType};
pub use overlay::{BLOCK_SIZE, Overlay};
