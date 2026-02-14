//! FUSE inode numbering and namespace management.
//!
//! This module defines the **inode numbering scheme** and **directory layout**
//! for the Hexz FUSE adapter. It provides the mapping between:
//! - Logical inode numbers (1, 2, 3)
//! - Snapshot streams (Disk, Memory)
//! - Directory entry names ("disk", "memory")
//! - File attributes (size, permissions, type)
//!
//! All inode-related constants and operations are centralized here to ensure
//! consistent behavior across lookup, getattr, and I/O operations.
//!
//! # Inode Numbering Scheme
//!
//! Hexz uses a **fixed inode layout** with only three possible inodes:
//!
//! | Inode | Type      | Name     | Backing         | Purpose                    |
//! |-------|-----------|----------|-----------------|----------------------------|
//! | 1     | Directory | (root)   | None            | Mount point root directory |
//! | 2     | File      | `disk`   | Disk stream     | Guest disk image           |
//! | 3     | File      | `memory` | Memory stream   | Guest RAM snapshot         |
//!
//! This minimal namespace is sufficient for unikernel snapshots, which consist
//! of a disk image and optional memory state. The root directory is always
//! present; `disk` and `memory` entries appear only if the corresponding
//! snapshot streams exist (determined by feature flags in the snapshot header).
//!
//! # InodeMap Structure
//!
//! The `InodeMap` struct caches snapshot metadata at mount time:
//! - Which streams are present (`has_disk`, `has_mem`)
//! - Stream sizes (`disk_size`, `mem_size`)
//! - Mount user/group IDs (`uid`, `gid`)
//!
//! This avoids repeated snapshot header queries and ensures attribute
//! consistency throughout the mount's lifetime. Changes to the underlying
//! snapshot (e.g., manual modification of the snapshot file) are not
//! reflected until remount.
//!
//! # Directory Entry Resolution
//!
//! The `lookup` method implements a simple name-to-inode mapping:
//! - Parent must be inode 1 (root)
//! - Name must be "disk" (if disk stream present) or "memory" (if memory present)
//! - All other names return `None`, causing FUSE to report `ENOENT`
//!
//! This flat namespace prevents nesting directories or creating new files,
//! keeping the filesystem read-mostly (writes only modify overlay data, not
//! the directory structure).
//!
//! # Lookup Performance
//!
//! - **Time complexity**: O(1) string comparison (at most 2 comparisons)
//! - **Typical latency**: 50-100 nanoseconds
//! - **No I/O**: All decisions based on in-memory `InodeMap` state
//!
//! The simplicity of the namespace ensures that directory operations never
//! become a bottleneck, even under high FUSE operation rates.
//!
//! # Examples
//!
//! ## Constructing an InodeMap
//!
//! ```no_run
//! use hexz_core::File;
//! use hexz_core::store::local::FileBackend;
//! use hexz_core::algo::compression::lz4::Lz4Compressor;
//! use hexz_fuse::vfs::InodeMap;
//! use std::sync::Arc;
//!
//! # fn main() -> anyhow::Result<()> {
//! let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
//! let compressor = Box::new(Lz4Compressor::new());
//! let snap = File::new(backend, compressor, None)?;
//! let inode_map = InodeMap::new(&snap, 1000, 1000);
//!
//! // Query available streams
//! if inode_map.lookup(1, "disk".as_ref()).is_some() {
//!     println!("Disk stream available");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Resolving Names to Inodes
//!
//! ```no_run
//! # use hexz_core::File;
//! # use hexz_core::store::local::FileBackend;
//! # use hexz_core::algo::compression::lz4::Lz4Compressor;
//! # use hexz_fuse::vfs::InodeMap;
//! # use std::sync::Arc;
//! # fn main() -> anyhow::Result<()> {
//! # let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
//! # let compressor = Box::new(Lz4Compressor::new());
//! # let snap = File::new(backend, compressor, None)?;
//! # let inode_map = InodeMap::new(&snap, 1000, 1000);
//! // Lookup "disk" under root
//! if let Some(ino) = inode_map.lookup(1, "disk".as_ref()) {
//!     assert_eq!(ino, 2);
//! }
//!
//! // Invalid parent
//! assert!(inode_map.lookup(2, "foo".as_ref()).is_none());
//!
//! // Unknown name
//! assert!(inode_map.lookup(1, "unknown".as_ref()).is_none());
//! # Ok(())
//! # }
//! ```

use fuser::FileType;
use hexz_core::{File, SnapshotStream};

use super::attr;

/// Logical inode identifier used by the FUSE adapter.
///
/// Inode numbers are 64-bit integers assigned by the filesystem. In Hexz,
/// they follow a fixed scheme:
/// - 1: Root directory
/// - 2: Disk file
/// - 3: Memory file
///
/// All other values are invalid. This type alias makes the intent clear in
/// function signatures while allowing efficient representation and comparison.
///
/// # FUSE Protocol Notes
///
/// FUSE inode numbers must be:
/// - Unique within a mount (satisfied by fixed assignment)
/// - Stable across lookups (satisfied by hardcoding)
/// - Non-zero (satisfied by starting at 1)
///
/// The kernel uses inode numbers as keys for its dentry cache, so consistency
/// is critical for correct path resolution.
pub type Inode = u64;

/// Fixed inode number assignments for the Hexz FUSE namespace.
///
/// This enum encodes the three possible inodes in the Hexz filesystem. The
/// `#[repr(u64)]` attribute ensures that each variant has a specific numeric
/// value that matches FUSE inode numbers.
///
/// # Inode Assignments
///
/// - **Root (1)**: The mount point directory, always present
/// - **Disk (2)**: The guest disk image, present if `has_disk` feature flag set
/// - **Memory (3)**: The guest RAM snapshot, present if `has_memory` feature flag set
///
/// # Usage
///
/// Convert between raw inode numbers and this enum using:
/// - `InodeType::Disk as u64` -> 2 (enum to raw)
/// - `InodeType::from_u64(2)` -> Some(InodeType::Disk) (raw to enum)
///
/// # Rationale
///
/// Using an enum (rather than raw constants) provides:
/// - Type safety in function signatures
/// - Exhaustive match checking when branching on inode type
/// - Self-documenting code (e.g., `ino == InodeType::Disk as u64`)
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InodeType {
    Root = 1,
    Disk = 2,
    Memory = 3,
}

impl InodeType {
    /// Converts a raw inode number into an `InodeType`, if valid.
    ///
    /// This method validates that an inode number falls within the expected
    /// range (1-3) and returns the corresponding enum variant. It is used when
    /// the FUSE kernel driver provides an inode number and the handler needs
    /// to determine its meaning.
    ///
    /// # Parameters
    ///
    /// - `v`: Raw inode number from FUSE operation (e.g., from `lookup` or `read`)
    ///
    /// # Returns
    ///
    /// - `Some(InodeType)`: If `v` is 1, 2, or 3
    /// - `None`: If `v` is any other value (invalid inode)
    ///
    /// # Examples
    ///
    /// ```
    /// use hexz_fuse::vfs::InodeType;
    ///
    /// assert_eq!(InodeType::from_u64(1), Some(InodeType::Root));
    /// assert_eq!(InodeType::from_u64(2), Some(InodeType::Disk));
    /// assert_eq!(InodeType::from_u64(3), Some(InodeType::Memory));
    /// assert_eq!(InodeType::from_u64(4), None);
    /// ```
    pub fn from_u64(v: u64) -> Option<Self> {
        match v {
            1 => Some(Self::Root),
            2 => Some(Self::Disk),
            3 => Some(Self::Memory),
            _ => None,
        }
    }
}

/// Directory entry returned by `readdir` operations.
///
/// This structure packages the metadata needed to populate the kernel's
/// directory cache during `readdir` iteration. Each entry corresponds to a
/// single file or subdirectory within the parent.
///
/// # Fields
///
/// - `inode`: The inode number of this entry (used for subsequent `getattr` calls)
/// - `kind`: File type (Directory or RegularFile) for `ls -F` and `stat` compatibility
/// - `name`: File name as a UTF-8 string (e.g., "disk", "memory", ".", "..")
///
/// # FUSE Protocol Notes
///
/// The `ReplyDirectory::add()` method consumes `DirEntry` values and encodes
/// them into the kernel-supplied buffer. If the buffer fills, `add()` returns
/// `true`, signaling that iteration should stop and resume on the next
/// `readdir` call with an updated offset.
///
/// # Examples
///
/// ```
/// use hexz_fuse::vfs::DirEntry;
/// use fuser::FileType;
///
/// let entry = DirEntry {
///     inode: 2,
///     kind: FileType::RegularFile,
///     name: "disk".to_string(),
/// };
///
/// assert_eq!(entry.name, "disk");
/// assert_eq!(entry.inode, 2);
/// ```
pub struct DirEntry {
    pub inode: Inode,
    pub kind: FileType,
    pub name: String,
}

/// In-memory cache of snapshot metadata for FUSE inode operations.
///
/// The `InodeMap` is constructed once at mount time and captures:
/// - Which snapshot streams (disk, memory) are present
/// - The logical size of each stream
/// - The UID/GID to assign to all inodes
///
/// This avoids repeatedly querying the snapshot header during FUSE operations
/// and ensures consistent attribute reporting throughout the mount's lifetime.
///
/// # Immutability
///
/// The `InodeMap` is **immutable after construction**. Changes to the
/// underlying snapshot file (e.g., manual edits with a hex editor) are not
/// reflected until the filesystem is unmounted and remounted.
///
/// For overlay modifications (writes to the disk inode), the FUSE adapter
/// queries overlay length dynamically via `Overlay::len()`, not via this map.
///
/// # Flat Namespace Assumption
///
/// The current implementation assumes a **flat namespace**:
/// - Root directory (inode 1) contains at most 2 files: `disk` and `memory`
/// - No subdirectories, no dynamic file creation
/// - Directory structure is fully determined by snapshot feature flags
///
/// This simplifies lookup and readdir operations to O(1) but prevents nesting
/// or runtime modification of the directory tree.
///
/// # Memory Footprint
///
/// - Size: 26 bytes (2 bools + 2 u64 + 2 u32, with padding)
/// - Lifetime: Same as FUSE mount (created in `Hexz::new`, dropped on unmount)
/// - Copies: Typically just one per mount (stored in `Hexz` struct)
pub struct InodeMap {
    has_disk: bool,
    has_mem: bool,
    disk_size: u64,
    mem_size: u64,
    uid: u32,
    gid: u32,
}

impl InodeMap {
    /// Constructs an `InodeMap` by reading snapshot metadata.
    ///
    /// This method queries the snapshot's feature flags and stream sizes to
    /// determine which inodes should be visible in the FUSE namespace. The
    /// resulting map is immutable and cached for the mount's lifetime.
    ///
    /// # Snapshot Header Inspection
    ///
    /// The following fields are extracted from `snap`:
    /// - `snap.header.features.has_disk`: Whether inode 2 (disk) should exist
    /// - `snap.header.features.has_memory`: Whether inode 3 (memory) should exist
    /// - `snap.size(SnapshotStream::Disk)`: Logical size of disk stream in bytes
    /// - `snap.size(SnapshotStream::Memory)`: Logical size of memory stream in bytes
    ///
    /// # Parameters
    ///
    /// - `snap`: Reference to an open snapshot file (must outlive the FUSE mount)
    /// - `uid`: User ID to assign to all inodes (typically from `getuid()`)
    /// - `gid`: Group ID to assign to all inodes (typically from `getgid()`)
    ///
    /// # Returns
    ///
    /// An `InodeMap` ready for use in `Hexz::new`. This map is cheap to
    /// clone (26 bytes) if needed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use hexz_core::File;
    /// use hexz_core::store::local::FileBackend;
    /// use hexz_core::algo::compression::lz4::Lz4Compressor;
    /// use hexz_fuse::vfs::InodeMap;
    /// use std::sync::Arc;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
    /// let compressor = Box::new(Lz4Compressor::new());
    /// let snap = File::new(backend, compressor, None)?;
    /// let inode_map = InodeMap::new(&snap, 1000, 1000);
    ///
    /// // Now inode_map can be used for lookups and attribute synthesis
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(snap: &File, uid: u32, gid: u32) -> Self {
        Self {
            has_disk: snap.header.features.has_disk,
            has_mem: snap.header.features.has_memory,
            disk_size: snap.size(SnapshotStream::Disk),
            mem_size: snap.size(SnapshotStream::Memory),
            uid,
            gid,
        }
    }

    /// Resolves a child name under a parent inode into an inode number.
    ///
    /// This method implements the core name-to-inode mapping used by the FUSE
    /// `lookup` operation. It validates that the parent is the root directory
    /// and that the name matches one of the exported files.
    ///
    /// # Lookup Rules
    ///
    /// 1. Parent must be 1 (root directory). Lookups under file inodes return `None`.
    /// 2. Name must be "disk" or "memory" (case-sensitive, UTF-8).
    /// 3. The corresponding snapshot stream must be present (checked via `has_disk`/`has_mem`).
    ///
    /// # Parameters
    ///
    /// - `parent`: Inode number of the directory being searched (must be 1)
    /// - `name`: Name to resolve (as `OsStr`, converted to UTF-8 internally)
    ///
    /// # Returns
    ///
    /// - `Some(2)`: If parent=1, name="disk", and disk stream present
    /// - `Some(3)`: If parent=1, name="memory", and memory stream present
    /// - `None`: Otherwise (invalid parent, unknown name, or stream not present)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use hexz_core::File;
    /// # use hexz_core::store::local::FileBackend;
    /// # use hexz_core::algo::compression::lz4::Lz4Compressor;
    /// # use hexz_fuse::vfs::InodeMap;
    /// # use std::sync::Arc;
    /// # fn main() -> anyhow::Result<()> {
    /// # let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
    /// # let compressor = Box::new(Lz4Compressor::new());
    /// # let snap = File::new(backend, compressor, None)?;
    /// # let map = InodeMap::new(&snap, 1000, 1000);
    /// // Valid lookup
    /// assert_eq!(map.lookup(1, "disk".as_ref()), Some(2));
    ///
    /// // Invalid parent
    /// assert_eq!(map.lookup(2, "disk".as_ref()), None);
    ///
    /// // Unknown name
    /// assert_eq!(map.lookup(1, "foo".as_ref()), None);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance
    ///
    /// - Time complexity: O(1) - two string comparisons maximum
    /// - Typical latency: 50-100 nanoseconds
    pub fn lookup(&self, parent: u64, name: &std::ffi::OsStr) -> Option<Inode> {
        if parent != InodeType::Root as u64 {
            return None;
        }
        let s = name.to_str()?;
        match s {
            "disk" if self.has_disk => Some(InodeType::Disk as u64),
            "memory" if self.has_mem => Some(InodeType::Memory as u64),
            _ => None,
        }
    }

    /// Synthesizes file attributes for a given inode.
    ///
    /// This method constructs a `FileAttr` structure by combining the inode
    /// number with snapshot-derived size information. It is used by the FUSE
    /// `getattr` and `lookup` operations to report file metadata to the kernel.
    ///
    /// # Attribute Sources
    ///
    /// - **Size**: Derived from `disk_size` or `mem_size` (cached at mount time)
    /// - **Permissions, timestamps, type**: Delegated to `attr::make_attr()`
    /// - **UID/GID**: From `uid` and `gid` (set at mount time)
    ///
    /// # Overlay Size Handling
    ///
    /// This method returns the **snapshot size**, not the overlay size. The
    /// FUSE adapter's `get_merged_attr()` method is responsible for merging
    /// overlay length with snapshot size when inode 2 (disk) is queried.
    ///
    /// # Parameters
    ///
    /// - `ino`: Inode number (1=root, 2=disk, 3=memory)
    ///
    /// # Returns
    ///
    /// A fully populated `FileAttr` structure. Unknown inodes receive size 0
    /// but still return valid attributes (this allows graceful degradation).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use hexz_core::File;
    /// # use hexz_core::store::local::FileBackend;
    /// # use hexz_core::algo::compression::lz4::Lz4Compressor;
    /// # use hexz_fuse::vfs::InodeMap;
    /// # use std::sync::Arc;
    /// # fn main() -> anyhow::Result<()> {
    /// # let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
    /// # let compressor = Box::new(Lz4Compressor::new());
    /// # let snap = File::new(backend, compressor, None)?;
    /// # let map = InodeMap::new(&snap, 1000, 1000);
    /// // Get attributes for disk inode
    /// let attr = map.getattr(2);
    /// assert_eq!(attr.ino, 2);
    /// // attr.size == snapshot disk size (not overlay size)
    /// # Ok(())
    /// # }
    /// ```
    pub fn getattr(&self, ino: u64) -> fuser::FileAttr {
        let size = match InodeType::from_u64(ino) {
            Some(InodeType::Disk) => self.disk_size,
            Some(InodeType::Memory) => self.mem_size,
            _ => 0,
        };

        attr::make_attr(ino, size, self.uid, self.gid)
    }

    /// Returns the complete directory listing for the root directory.
    ///
    /// This method constructs the set of directory entries visible in the root
    /// directory (inode 1). The entries are materialized on demand from the
    /// snapshot feature flags; no on-disk directory structure is consulted.
    ///
    /// # Entry Ordering
    ///
    /// The returned vector always follows this order:
    /// 1. `.` (current directory, inode 1)
    /// 2. `..` (parent directory, also inode 1 since root has no parent)
    /// 3. `disk` (inode 2, if disk stream present)
    /// 4. `memory` (inode 3, if memory stream present)
    ///
    /// This ordering is **stable** and can be relied upon for testing, but the
    /// FUSE protocol does not require any specific order.
    ///
    /// # Returns
    ///
    /// A vector of `DirEntry` structures, each containing:
    /// - `inode`: Inode number for subsequent operations
    /// - `kind`: File type (Directory for `.` and `..`, RegularFile otherwise)
    /// - `name`: UTF-8 file name
    ///
    /// The vector length is 2-4 entries depending on snapshot contents.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use hexz_core::File;
    /// # use hexz_core::store::local::FileBackend;
    /// # use hexz_core::algo::compression::lz4::Lz4Compressor;
    /// # use hexz_fuse::vfs::InodeMap;
    /// # use std::sync::Arc;
    /// # fn main() -> anyhow::Result<()> {
    /// # let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
    /// # let compressor = Box::new(Lz4Compressor::new());
    /// # let snap = File::new(backend, compressor, None)?;
    /// # let map = InodeMap::new(&snap, 1000, 1000);
    /// let entries = map.readdir();
    ///
    /// // Minimum entries: . and ..
    /// assert!(entries.len() >= 2);
    /// assert_eq!(entries[0].name, ".");
    /// assert_eq!(entries[1].name, "..");
    ///
    /// // Additional entries if streams present
    /// if map.lookup(1, "disk".as_ref()).is_some() {
    ///     assert_eq!(entries[2].name, "disk");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance
    ///
    /// - Time complexity: O(1) - fixed entry count (2-4)
    /// - Space complexity: O(1) - fixed allocation
    /// - Typical latency: < 1 microsecond
    pub fn readdir(&self) -> Vec<DirEntry> {
        let mut entries = vec![
            DirEntry {
                inode: InodeType::Root as u64,
                kind: FileType::Directory,
                name: ".".into(),
            },
            DirEntry {
                inode: InodeType::Root as u64,
                kind: FileType::Directory,
                name: "..".into(),
            },
        ];
        if self.has_disk {
            entries.push(DirEntry {
                inode: InodeType::Disk as u64,
                kind: FileType::RegularFile,
                name: "disk".into(),
            });
        }
        if self.has_mem {
            entries.push(DirEntry {
                inode: InodeType::Memory as u64,
                kind: FileType::RegularFile,
                name: "memory".into(),
            });
        }
        entries
    }

    /// Maps an inode number to its backing snapshot stream.
    ///
    /// This method is used by read operations to determine which snapshot
    /// stream should service a read request. It validates that the inode
    /// corresponds to a file (not a directory) and that the corresponding
    /// snapshot stream exists.
    ///
    /// # Mapping Rules
    ///
    /// - Inode 1 (root): Returns `None` (directories have no stream)
    /// - Inode 2 (disk): Returns `Some(SnapshotStream::Disk)` if `has_disk`
    /// - Inode 3 (memory): Returns `Some(SnapshotStream::Memory)` if `has_mem`
    /// - Other inodes: Returns `None` (invalid)
    ///
    /// # Parameters
    ///
    /// - `ino`: Inode number from a FUSE operation
    ///
    /// # Returns
    ///
    /// - `Some(SnapshotStream)`: If the inode maps to a valid, present stream
    /// - `None`: If the inode is invalid or corresponds to a directory
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use hexz_core::File;
    /// # use hexz_core::store::local::FileBackend;
    /// # use hexz_core::algo::compression::lz4::Lz4Compressor;
    /// # use hexz_core::SnapshotStream;
    /// # use hexz_fuse::vfs::InodeMap;
    /// # use std::sync::Arc;
    /// # fn main() -> anyhow::Result<()> {
    /// # let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
    /// # let compressor = Box::new(Lz4Compressor::new());
    /// # let snap = File::new(backend, compressor, None)?;
    /// # let map = InodeMap::new(&snap, 1000, 1000);
    /// // Valid stream mapping
    /// assert_eq!(map.inode_to_stream(2), Some(SnapshotStream::Disk));
    ///
    /// // Root directory has no stream
    /// assert_eq!(map.inode_to_stream(1), None);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance
    ///
    /// - Time complexity: O(1) - enum match
    /// - Typical latency: < 10 nanoseconds
    pub fn inode_to_stream(&self, ino: u64) -> Option<SnapshotStream> {
        match InodeType::from_u64(ino) {
            Some(InodeType::Disk) if self.has_disk => Some(SnapshotStream::Disk),
            Some(InodeType::Memory) if self.has_mem => Some(SnapshotStream::Memory),
            _ => None,
        }
    }
}
