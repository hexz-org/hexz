//! FUSE inode layout and attribute synthesis.
//!
//! Defines the mapping from logical inode numbers to snapshot streams (root,
//! disk, memory), directory entries, and synthesized file attributes. All
//! inode arithmetic and permission constants are centralized here so the
//! FUSE adapter remains a thin wrapper over snapshot and overlay I/O.

use fuser::FileType;
use strata_core::{SnapshotStream, StrataFile};

use super::attr;

/// Logical inode identifier used by the FUSE adapter.
///
/// **Architectural intent:** Keeps inode arithmetic simple by using a plain
/// `u64` while reserving low values for well-known entries (root, disk,
/// memory).
///
/// **Constraints:** The mapping from inode values to meaning is fixed in this
/// module; external callers must treat inodes as opaque.
pub type Inode = u64;

/// Discriminant for the minimal FUSE namespace (root directory and streams).
///
/// **Architectural intent:** Encodes the fixed inode numbering used by the
/// FUSE adapter so that lookups and getattr can map inode numbers to
/// snapshot streams or directory semantics without external configuration.
///
/// **Constraints:** Values 1–3 are reserved; other inodes are invalid and
/// must not be returned by the adapter.
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InodeType {
    Root = 1,
    Disk = 2,
    Memory = 3,
}

impl InodeType {
    /// Converts a raw FUSE inode number into an `InodeType`, if valid.
    ///
    /// **Architectural intent:** Bridges kernel-supplied inode values with
    /// the internal discriminant so that handlers can branch on stream type
    /// without duplicating magic numbers.
    ///
    /// **Constraints:** Returns `None` for any value not in 1..=3; callers
    /// must handle `None` as an invalid or unknown inode.
    pub fn from_u64(v: u64) -> Option<Self> {
        match v {
            1 => Some(Self::Root),
            2 => Some(Self::Disk),
            3 => Some(Self::Memory),
            _ => None,
        }
    }
}

/// Directory entry description returned to the FUSE layer.
///
/// **Architectural intent:** Packages the inode number, file type, and name
/// into a single value that can be streamed into `ReplyDirectory`.
///
/// **Constraints:** `inode` values must be consistent with those used by
/// `InodeMap`; mismatches will surface as lookup or getattr inconsistencies.
pub struct DirEntry {
    pub inode: Inode,
    pub kind: FileType,
    pub name: String,
}

/// Snapshot-derived view of the tiny exported filesystem.
///
/// **Architectural intent:** Encodes which logical streams are present in a
/// snapshot and their sizes, and provides the mapping from names to inodes
/// used by the FUSE adapter.
///
/// **Constraints:** Assumes a flat namespace under the root directory with
/// at most two regular files: `disk` and `memory`.
pub struct InodeMap {
    has_disk: bool,
    has_mem: bool,
    disk_size: u64,
    mem_size: u64,
    uid: u32,
    gid: u32,
}

impl InodeMap {
    /// Constructs an inode map from a `StrataFile` header and sizes.
    ///
    /// **Architectural intent:** Captures the presence and logical length of
    /// disk and memory streams at mount time so later attribute queries do not
    /// need to touch the snapshot backend.
    ///
    /// **Constraints:** The map is not updated after creation; changes to the
    /// underlying snapshot are not reflected until remount.
    pub fn new(snap: &StrataFile, uid: u32, gid: u32) -> Self {
        Self {
            has_disk: snap.header.features.has_disk,
            has_mem: snap.header.features.has_memory,
            disk_size: snap.size(SnapshotStream::Disk),
            mem_size: snap.size(SnapshotStream::Memory),
            uid,
            gid,
        }
    }

    /// Resolves a child name under the root directory into an inode.
    ///
    /// **Architectural intent:** Provides a deterministic mapping from the
    /// exported names `disk` and `memory` to their reserved inode numbers.
    ///
    /// **Constraints:** Only lookups with `parent == 1` (the root) are
    /// supported; all other parents return `None`.
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

    /// Synthesizes `FileAttr` for a given inode using snapshot sizes.
    ///
    /// **Architectural intent:** Provides minimal metadata required by FUSE:
    /// size, block count, timestamps, permissions, and file type.
    ///
    /// **Constraints:** Uses fixed permissions, user, and group IDs; callers
    /// should not rely on these for security isolation.
    pub fn getattr(&self, ino: u64) -> fuser::FileAttr {
        let size = match InodeType::from_u64(ino) {
            Some(InodeType::Disk) => self.disk_size,
            Some(InodeType::Memory) => self.mem_size,
            _ => 0,
        };

        attr::make_attr(ino, size, self.uid, self.gid)
    }

    /// Returns the directory listing for the root inode.
    ///
    /// **Architectural intent:** Materializes `.` and `..` plus `disk` and
    /// optionally `memory` entries, based on feature flags.
    ///
    /// **Constraints:** The ordering of entries is stable but not significant;
    /// callers must not assume additional files will appear here.
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

    /// Maps an inode back to its corresponding `SnapshotStream`, if any.
    ///
    /// **Architectural intent:** Bridges the FUSE inode space and the logical
    /// snapshot streams so that the adapter can choose the correct reader.
    ///
    /// **Constraints:** Only inodes `2` and `3` may map to streams and only
    /// when the corresponding feature flags were set at construction time.
    pub fn inode_to_stream(&self, ino: u64) -> Option<SnapshotStream> {
        match InodeType::from_u64(ino) {
            Some(InodeType::Disk) if self.has_disk => Some(SnapshotStream::Disk),
            Some(InodeType::Memory) if self.has_mem => Some(SnapshotStream::Memory),
            _ => None,
        }
    }
}
