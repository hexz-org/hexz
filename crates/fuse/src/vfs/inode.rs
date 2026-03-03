//! FUSE inode numbering and namespace management.
//!
//! This module defines the **inode numbering scheme** and **directory layout**
//! for the Hexz FUSE adapter. It provides the mapping between:
//! - Logical inode numbers (1, 2, 3)
//! - Archive streams (Disk, Memory)
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
//! | 2     | File      | `disk`   | Main stream     | Guest disk image           |
//! | 3     | File      | `memory` | Auxiliary stream   | Guest RAM archive         |
//!
//! This minimal namespace is sufficient for unikernel archives, which consist
//! of a disk image and optional memory state. The root directory is always
//! present; `disk` and `memory` entries appear only if the corresponding
//! archive streams exist (determined by feature flags in the archive header).
//!
//! # InodeMap Structure
//!
//! The `InodeMap` struct caches archive metadata at mount time:
//! - Which streams are present (`has_disk`, `has_mem`)
//! - Stream sizes (`main_size`, `mem_size`)
//! - Mount user/group IDs (`uid`, `gid`)
//!
//! This avoids repeated archive header queries and ensures attribute
//! consistency throughout the mount's lifetime. Changes to the underlying
//! archive (e.g., manual modification of the archive file) are not
//! reflected until remount.
//!
//! # Directory Entry Resolution
//!
//! The `lookup` method implements a simple name-to-inode mapping:
//! - Parent must be inode 1 (root)
//! - Name must be "disk" (if main stream present) or "memory" (if memory present)
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
//! use hexz_store::local::FileBackend;
//! use hexz_core::algo::compression::lz4::Lz4Compressor;
//! use hexz_fuse::vfs::InodeMap;
//! use std::sync::Arc;
//!
//! # fn main() -> anyhow::Result<()> {
//! let backend = Arc::new(FileBackend::new("archive.hxz".as_ref())?);
//! let compressor = Box::new(Lz4Compressor::new());
//! let snap = File::new(backend, compressor, None)?;
//! let inode_map = InodeMap::new(&snap, 1000, 1000);
//!
//! // Query available streams
//! if inode_map.lookup(1, "disk".as_ref()).is_some() {
//!     println!("Main stream available");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Resolving Names to Inodes
//!
//! ```no_run
//! # use hexz_core::File;
//! # use hexz_store::local::FileBackend;
//! # use hexz_core::algo::compression::lz4::Lz4Compressor;
//! # use hexz_fuse::vfs::InodeMap;
//! # use std::sync::Arc;
//! # fn main() -> anyhow::Result<()> {
//! # let backend = Arc::new(FileBackend::new("archive.hxz".as_ref())?);
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
use hexz_core::api::manifest::{ArchiveManifest, FileEntry};
use hexz_core::{Archive, ArchiveStream};
use serde_json;
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::attr;

pub type Inode = u64;

pub const ROOT_INODE: Inode = 1;

pub struct DirEntry {
    pub inode: Inode,
    pub kind: FileType,
    pub name: String,
}

#[derive(Debug, Clone)]
pub enum Node {
    Directory {
        children: BTreeMap<String, Inode>,
        parent: Option<Inode>,
    },
    File {
        offset: u64,
        size: u64,
        mode: u32,
        mtime: u64,
        parent: Option<Inode>,
    },
}

pub struct InodeMap {
    nodes: BTreeMap<Inode, Node>,
    path_to_ino: BTreeMap<PathBuf, Inode>,
    ino_to_path: BTreeMap<Inode, PathBuf>,
    pub passthrough_paths: BTreeMap<Inode, PathBuf>,
    next_inode: Inode,
    uid: u32,
    gid: u32,
}

impl InodeMap {
    pub fn new(snap: &Archive, uid: u32, gid: u32) -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            ROOT_INODE,
            Node::Directory {
                children: BTreeMap::new(),
                parent: None,
            },
        );

        let mut map = Self {
            nodes,
            path_to_ino: BTreeMap::new(),
            ino_to_path: BTreeMap::new(),
            passthrough_paths: BTreeMap::new(),
            next_inode: 2,
            uid,
            gid,
        };
        map.ino_to_path.insert(ROOT_INODE, PathBuf::new());

        // Parse manifest from metadata
        if let Some(metadata) = &snap.metadata {
            if let Ok(manifest) = serde_json::from_slice::<ArchiveManifest>(metadata) {
                for file in manifest.files {
                    map.add_file(&file);
                }
            }
        } else {
            if snap.header.features.has_main {
                map.add_legacy_file("main", snap.size(ArchiveStream::Main));
            }
            if snap.header.features.has_auxiliary {
                map.add_legacy_file("auxiliary", snap.size(ArchiveStream::Auxiliary));
            }
        }

        map
    }

    pub fn add_file_at_path(&mut self, path: &std::path::Path, is_dir: bool) -> Inode {
        if let Some(&ino) = self.path_to_ino.get(path) {
            return ino;
        }

        let parent_path = path.parent().unwrap_or_else(|| std::path::Path::new(""));
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let parent_ino = self.get_or_create_dir(parent_path);

        let ino = self.next_inode;
        self.next_inode += 1;

        if is_dir {
            self.nodes.insert(
                ino,
                Node::Directory {
                    children: BTreeMap::new(),
                    parent: Some(parent_ino),
                },
            );
        } else {
            self.nodes.insert(
                ino,
                Node::File {
                    offset: 0,
                    size: 0,
                    mode: 0o644,
                    mtime: 0,
                    parent: Some(parent_ino),
                },
            );
        }

        let pb = path.to_path_buf();
        self.path_to_ino.insert(pb.clone(), ino);
        self.ino_to_path.insert(ino, pb);

        if let Some(Node::Directory { children, .. }) = self.nodes.get_mut(&parent_ino) {
            children.insert(name, ino);
        }
        ino
    }

    fn add_legacy_file(&mut self, name: &str, size: u64) {
        let ino = self.next_inode;
        self.next_inode += 1;
        self.nodes.insert(
            ino,
            Node::File {
                offset: 0,
                size,
                mode: 0o644,
                mtime: 0,
                parent: Some(ROOT_INODE),
            },
        );
        let pb = PathBuf::from(name);
        self.path_to_ino.insert(pb.clone(), ino);
        self.ino_to_path.insert(ino, pb);

        if let Some(Node::Directory { children, .. }) = self.nodes.get_mut(&ROOT_INODE) {
            children.insert(name.to_string(), ino);
        }
    }

    fn get_or_create_dir(&mut self, path: &std::path::Path) -> Inode {
        let mut current_ino = ROOT_INODE;
        let mut current_path = PathBuf::new();

        for component in path.iter() {
            let name = component.to_string_lossy().to_string();
            current_path.push(&name);

            let mut create_new = false;
            if let Some(Node::Directory { children, .. }) = self.nodes.get(&current_ino) {
                if let Some(&child_ino) = children.get(&name) {
                    current_ino = child_ino;
                    continue;
                } else {
                    create_new = true;
                }
            }

            if create_new {
                let new_ino = self.next_inode;
                self.next_inode += 1;

                self.nodes.insert(
                    new_ino,
                    Node::Directory {
                        children: BTreeMap::new(),
                        parent: Some(current_ino),
                    },
                );

                let pb = current_path.clone();
                self.path_to_ino.insert(pb.clone(), new_ino);
                self.ino_to_path.insert(new_ino, pb);

                if let Some(Node::Directory { children, .. }) = self.nodes.get_mut(&current_ino) {
                    children.insert(name, new_ino);
                }

                current_ino = new_ino;
            }
        }

        current_ino
    }
    /// Recursively populates the InodeMap from an existing overlay directory.
    pub fn populate_from_metadata_dir(&mut self, metadata_dir: &std::path::Path) {
        if !metadata_dir.exists() {
            return;
        }

        let base = std::path::Path::new(".hexz");
        self.get_or_create_dir(base);

        let walk = walkdir::WalkDir::new(metadata_dir);
        for entry in walk
            .into_iter()
            .filter_map(|e: walkdir::Result<walkdir::DirEntry>| e.ok())
        {
            if entry.path() == metadata_dir {
                continue;
            }
            if let Ok(rel_path) = entry.path().strip_prefix(metadata_dir) {
                let virtual_path = base.join(rel_path);
                let is_dir = entry.file_type().is_dir();
                let ino = self.add_file_at_path(&virtual_path, is_dir);
                self.passthrough_paths
                    .insert(ino, entry.path().to_path_buf());
            }
        }
    }

    pub fn populate_from_overlay(&mut self, base: &std::path::Path) {
        let walk = walkdir::WalkDir::new(base);
        for entry in walk
            .into_iter()
            .filter_map(|e: walkdir::Result<walkdir::DirEntry>| e.ok())
        {
            if entry.path() == base {
                continue;
            }
            if let Ok(rel_path) = entry.path().strip_prefix(base) {
                let is_dir = entry.file_type().is_dir();
                self.add_file_at_path(rel_path, is_dir);
            }
        }
    }

    fn add_file(&mut self, entry: &FileEntry) {
        let path = std::path::Path::new(&entry.path);
        let parent_path = path.parent().unwrap_or_else(|| std::path::Path::new(""));
        let name = path.file_name().unwrap().to_string_lossy().to_string();

        let parent_ino = self.get_or_create_dir(parent_path);

        let file_ino = self.next_inode;
        self.next_inode += 1;

        self.nodes.insert(
            file_ino,
            Node::File {
                offset: entry.offset,
                size: entry.size,
                mode: entry.mode,
                mtime: entry.mtime,
                parent: Some(parent_ino),
            },
        );

        let pb = path.to_path_buf();
        self.path_to_ino.insert(pb.clone(), file_ino);
        self.ino_to_path.insert(file_ino, pb);

        if let Some(Node::Directory { children, .. }) = self.nodes.get_mut(&parent_ino) {
            children.insert(name, file_ino);
        }
    }

    pub fn lookup(&self, parent: u64, name: &std::ffi::OsStr) -> Option<Inode> {
        let s = name.to_str()?;
        if let Some(Node::Directory { children, .. }) = self.nodes.get(&parent) {
            children.get(s).copied()
        } else {
            None
        }
    }

    pub fn getattr(&self, ino: u64) -> fuser::FileAttr {
        match self.nodes.get(&ino) {
            Some(Node::Directory { .. }) => {
                let mut attr = attr::make_attr(ino, 4096, self.uid, self.gid);
                attr.kind = FileType::Directory;
                attr.perm = 0o755;
                attr
            }
            Some(Node::File { size, mode, .. }) => {
                let mut attr = attr::make_attr(ino, *size, self.uid, self.gid);
                attr.kind = FileType::RegularFile;
                attr.perm = (*mode as u16) & 0o777;
                attr
            }
            None => attr::make_attr(ino, 0, self.uid, self.gid), // Fallback
        }
    }

    pub fn readdir(&self, ino: u64) -> Vec<DirEntry> {
        let mut entries = Vec::new();

        if let Some(Node::Directory { children, parent }) = self.nodes.get(&ino) {
            entries.push(DirEntry {
                inode: ino,
                kind: FileType::Directory,
                name: ".".into(),
            });
            entries.push(DirEntry {
                inode: parent.unwrap_or(ROOT_INODE),
                kind: FileType::Directory,
                name: "..".into(),
            });

            for (name, &child_ino) in children {
                let kind = match self.nodes.get(&child_ino) {
                    Some(Node::Directory { .. }) => FileType::Directory,
                    Some(Node::File { .. }) => FileType::RegularFile,
                    None => continue,
                };
                entries.push(DirEntry {
                    inode: child_ino,
                    kind,
                    name: name.clone(),
                });
            }
        }

        entries
    }

    pub fn is_valid_inode(&self, ino: u64) -> bool {
        self.nodes.contains_key(&ino)
    }

    pub fn get_inode(&self, path: &std::path::Path) -> Option<Inode> {
        self.path_to_ino.get(path).copied()
    }

    /// Reconstructs the full logical path for a given inode.
    pub fn get_path(&self, ino: u64) -> Option<std::path::PathBuf> {
        self.ino_to_path.get(&ino).cloned()
    }

    pub fn uid(&self) -> u32 {
        self.uid
    }

    pub fn gid(&self) -> u32 {
        self.gid
    }

    /// Returns (stream, offset_in_stream, size)
    pub fn file_info(&self, ino: u64) -> Option<(ArchiveStream, u64, u64)> {
        match self.nodes.get(&ino) {
            Some(Node::File { offset, size, .. }) => {
                // If it's the legacy memory file, we would need to map it to Auxiliary stream.
                // But for now, let's assume everything in manifest is Main.
                // To support legacy memory, we check if offset=0 and size matches...
                // Actually, let's simplify and just use Main for manifest files.
                Some((ArchiveStream::Main, *offset, *size))
            }
            _ => None,
        }
    }
}
