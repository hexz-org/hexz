#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, unused_results))]

//! Platform-agnostic virtual filesystem logic for Hexz.

use fuser::FileType;
use hexz_core::api::manifest::{ArchiveManifest, FileEntry};
use hexz_core::{Archive, ArchiveStream};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// A unique identifier for a node in the virtual filesystem.
pub type Inode = u64;

/// The inode number of the root directory.
pub const ROOT_INODE: Inode = 1;

/// Permission bits for the root directory (owner rwx, group/other rx).
pub const PERM_DIR: u16 = 0o755;

/// Permission bits for regular files (owner rw, group/other r).
pub const PERM_FILE: u16 = 0o644;

/// Block size in bytes reported for `stat` block counts (512).
pub const VFS_BLOCK_SIZE: u64 = 512;

/// A single entry within a directory listing.
#[derive(Debug)]
pub struct DirEntry {
    /// The inode number of this entry.
    pub inode: Inode,
    /// The file type (directory, regular file, etc.).
    pub kind: FileType,
    /// The name of this entry within its parent directory.
    pub name: String,
}

/// A node in the virtual filesystem tree.
#[derive(Debug, Clone)]
pub enum Node {
    /// A directory node containing named children.
    Directory {
        /// Map from child name to child inode.
        children: BTreeMap<String, Inode>,
        /// The parent directory's inode, if any.
        parent: Option<Inode>,
    },
    /// A regular file node backed by archive data.
    File {
        /// Byte offset of this file's data within the archive stream.
        offset: u64,
        /// Size of the file in bytes.
        size: u64,
        /// Unix file mode bits.
        mode: u32,
        /// Modification time as seconds since the Unix epoch.
        mtime: u64,
        /// The parent directory's inode, if any.
        parent: Option<Inode>,
    },
}

/// Attributes of a virtual filesystem node, analogous to `stat`.
#[derive(Debug, Clone)]
pub struct VfsAttr {
    /// Inode number.
    pub ino: Inode,
    /// File size in bytes.
    pub size: u64,
    /// Number of 512-byte blocks allocated.
    pub blocks: u64,
    /// Last access time.
    pub atime: std::time::SystemTime,
    /// Last modification time.
    pub mtime: std::time::SystemTime,
    /// Last status change time.
    pub ctime: std::time::SystemTime,
    /// Creation time.
    pub crtime: std::time::SystemTime,
    /// File type (directory, regular file, etc.).
    pub kind: FileType,
    /// Permission bits.
    pub perm: u16,
    /// Number of hard links.
    pub nlink: u32,
    /// Owner user ID.
    pub uid: u32,
    /// Owner group ID.
    pub gid: u32,
    /// Device ID (for special files).
    pub rdev: u32,
    /// File flags.
    pub flags: u32,
    /// Preferred I/O block size.
    pub blksize: u32,
}

/// Creates a default [`VfsAttr`] for the given inode and size.
pub const fn make_attr(ino: Inode, size: u64, uid: u32, gid: u32) -> VfsAttr {
    VfsAttr {
        ino,
        size,
        blocks: size.div_ceil(VFS_BLOCK_SIZE),
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: if ino == ROOT_INODE {
            FileType::Directory
        } else {
            FileType::RegularFile
        },
        perm: if ino == ROOT_INODE {
            PERM_DIR
        } else {
            PERM_FILE
        },
        nlink: if ino == ROOT_INODE { 2 } else { 1 },
        uid,
        gid,
        rdev: 0,
        flags: 0,
        blksize: VFS_BLOCK_SIZE as u32,
    }
}

/// Maps inodes to virtual filesystem nodes, providing path-based lookups and
/// directory traversal for a mounted archive.
pub struct InodeMap {
    nodes: BTreeMap<Inode, Node>,
    path_to_ino: BTreeMap<PathBuf, Inode>,
    ino_to_path: BTreeMap<Inode, PathBuf>,
    /// Maps inodes to real filesystem paths for passthrough reads.
    pub passthrough_paths: BTreeMap<Inode, PathBuf>,
    next_inode: Inode,
    uid: u32,
    gid: u32,
}

impl std::fmt::Debug for InodeMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InodeMap")
            .field("nodes", &self.nodes)
            .field("path_to_ino", &self.path_to_ino)
            .field("ino_to_path", &self.ino_to_path)
            .field("passthrough_paths", &self.passthrough_paths)
            .field("next_inode", &self.next_inode)
            .field("uid", &self.uid)
            .field("gid", &self.gid)
            .finish()
    }
}

impl InodeMap {
    /// Creates a new `InodeMap` populated from the given archive snapshot.
    pub fn new(snap: &Archive, uid: u32, gid: u32) -> Self {
        let mut nodes = BTreeMap::new();
        _ = nodes.insert(
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
        _ = map.ino_to_path.insert(ROOT_INODE, PathBuf::new());

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

    /// Registers a file or directory at the given path, creating parent directories as needed.
    /// Returns the inode assigned to the path.
    pub fn add_file_at_path(&mut self, path: &Path, is_dir: bool) -> Inode {
        if let Some(&ino) = self.path_to_ino.get(path) {
            return ino;
        }

        let parent_path = path.parent().unwrap_or_else(|| Path::new(""));
        let Some(name) = path.file_name() else {
            return ROOT_INODE;
        };
        let name = name.to_string_lossy().to_string();
        let parent_ino = self.get_or_create_dir(parent_path);

        let ino = self.next_inode;
        self.next_inode += 1;

        if is_dir {
            _ = self.nodes.insert(
                ino,
                Node::Directory {
                    children: BTreeMap::new(),
                    parent: Some(parent_ino),
                },
            );
        } else {
            _ = self.nodes.insert(
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
        _ = self.path_to_ino.insert(pb.clone(), ino);
        _ = self.ino_to_path.insert(ino, pb);

        if let Some(Node::Directory { children, .. }) = self.nodes.get_mut(&parent_ino) {
            _ = children.insert(name, ino);
        }
        ino
    }

    fn add_legacy_file(&mut self, name: &str, size: u64) {
        let ino = self.next_inode;
        self.next_inode += 1;
        _ = self.nodes.insert(
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
        _ = self.path_to_ino.insert(pb.clone(), ino);
        _ = self.ino_to_path.insert(ino, pb);

        if let Some(Node::Directory { children, .. }) = self.nodes.get_mut(&ROOT_INODE) {
            _ = children.insert(name.to_string(), ino);
        }
    }

    fn get_or_create_dir(&mut self, path: &Path) -> Inode {
        let mut current_ino = ROOT_INODE;
        let mut current_path = PathBuf::new();

        for component in path {
            let name = component.to_string_lossy().to_string();
            current_path.push(&name);

            let create_new = if let Some(Node::Directory { children, .. }) = self.nodes.get(&current_ino) {
                if let Some(&child_ino) = children.get(&name) {
                    current_ino = child_ino;
                    continue;
                }
                true
            } else {
                false
            };

            if create_new {
                let new_ino = self.next_inode;
                self.next_inode += 1;

                _ = self.nodes.insert(
                    new_ino,
                    Node::Directory {
                        children: BTreeMap::new(),
                        parent: Some(current_ino),
                    },
                );

                let pb = current_path.clone();
                _ = self.path_to_ino.insert(pb.clone(), new_ino);
                _ = self.ino_to_path.insert(new_ino, pb);

                if let Some(Node::Directory { children, .. }) = self.nodes.get_mut(&current_ino) {
                    _ = children.insert(name, new_ino);
                }

                current_ino = new_ino;
            }
        }

        current_ino
    }

    /// Populates the virtual `.hexz` metadata directory from real filesystem entries.
    pub fn populate_from_metadata_dir(&mut self, metadata_dir: &Path) {
        if !metadata_dir.exists() {
            return;
        }

        let base = Path::new(".hexz");
        _ = self.get_or_create_dir(base);

        let walk = walkdir::WalkDir::new(metadata_dir);
        for entry in walk
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            if entry.path() == metadata_dir {
                continue;
            }
            if let Ok(rel_path) = entry.path().strip_prefix(metadata_dir) {
                let virtual_path = base.join(rel_path);
                let is_dir = entry.file_type().is_dir();
                let ino = self.add_file_at_path(&virtual_path, is_dir);
                _ = self.passthrough_paths
                    .insert(ino, entry.path().to_path_buf());
            }
        }
    }

    /// Populates the virtual filesystem from an overlay directory on the real filesystem.
    pub fn populate_from_overlay(&mut self, base: &Path) {
        let walk = walkdir::WalkDir::new(base);
        for entry in walk
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            if entry.path() == base {
                continue;
            }
            if let Ok(rel_path) = entry.path().strip_prefix(base) {
                let is_dir = entry.file_type().is_dir();
                _ = self.add_file_at_path(rel_path, is_dir);
            }
        }
    }

    fn add_file(&mut self, entry: &FileEntry) {
        let path = Path::new(&entry.path);
        let parent_path = path.parent().unwrap_or_else(|| Path::new(""));
        let Some(name) = path.file_name() else {
            return;
        };
        let name = name.to_string_lossy().to_string();

        let parent_ino = self.get_or_create_dir(parent_path);

        let file_ino = self.next_inode;
        self.next_inode += 1;

        _ = self.nodes.insert(
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
        _ = self.path_to_ino.insert(pb.clone(), file_ino);
        _ = self.ino_to_path.insert(file_ino, pb);

        if let Some(Node::Directory { children, .. }) = self.nodes.get_mut(&parent_ino) {
            _ = children.insert(name, file_ino);
        }
    }

    /// Looks up a child entry by name within the given parent directory.
    pub fn lookup(&self, parent: u64, name: &std::ffi::OsStr) -> Option<Inode> {
        let s = name.to_str()?;
        if let Some(Node::Directory { children, .. }) = self.nodes.get(&parent) {
            children.get(s).copied()
        } else {
            None
        }
    }

    /// Returns `true` if the given inode exists in this map.
    pub fn is_valid_inode(&self, ino: u64) -> bool {
        self.nodes.contains_key(&ino)
    }

    /// Returns the inode for the given path, if it exists.
    pub fn get_inode(&self, path: &Path) -> Option<Inode> {
        self.path_to_ino.get(path).copied()
    }

    /// Returns the path associated with the given inode, if it exists.
    pub fn get_path(&self, ino: u64) -> Option<PathBuf> {
        self.ino_to_path.get(&ino).cloned()
    }

    /// Renames a path and updates all descendant paths accordingly.
    /// Returns `true` if the rename succeeded.
    pub fn rename_path(&mut self, old_path: &Path, new_path: &Path) -> bool {
        if let Some(ino) = self.path_to_ino.remove(old_path) {
            _ = self.path_to_ino.insert(new_path.to_path_buf(), ino);
            _ = self.ino_to_path.insert(ino, new_path.to_path_buf());

            // If it's a directory, we need to update all children
            // This is simplified: in a real VFS we'd walk the tree,
            // but for hexz shell we can just clear child caches or
            // rely on the fact that rename is usually for files.
            // Let's do a basic child update.
            let old_prefix = old_path.to_path_buf();
            let mut to_update = Vec::new();
            for (path, &child_ino) in &self.path_to_ino {
                if path.starts_with(&old_prefix) {
                    to_update.push((path.clone(), child_ino));
                }
            }
            for (old_child_path, child_ino) in to_update {
                if let Ok(rel) = old_child_path.strip_prefix(&old_prefix) {
                    let new_child_path = new_path.join(rel);
                    _ = self.path_to_ino.remove(&old_child_path);
                    _ = self.path_to_ino.insert(new_child_path.clone(), child_ino);
                    _ = self.ino_to_path.insert(child_ino, new_child_path);
                }
            }
            return true;
        }
        false
    }

    /// Removes the given path from the inode map.
    pub fn remove_path(&mut self, path: &Path) {
        if let Some(ino) = self.path_to_ino.remove(path) {
            // We keep the ino_to_path for a bit to avoid immediate reuse
            // but effectively the path is gone.
            _ = self.ino_to_path.remove(&ino);
        }
    }

    /// Returns the owner user ID.
    pub const fn uid(&self) -> u32 {
        self.uid
    }

    /// Returns the owner group ID.
    pub const fn gid(&self) -> u32 {
        self.gid
    }

    /// Returns the archive stream, offset, and size for the given file inode.
    pub fn file_info(&self, ino: u64) -> Option<(ArchiveStream, u64, u64)> {
        match self.nodes.get(&ino) {
            Some(Node::File { offset, size, .. }) => {
                Some((ArchiveStream::Main, *offset, *size))
            }
            _ => None,
        }
    }

    /// Returns the directory listing for the given directory inode.
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

    /// Returns the filesystem attributes for the given inode.
    pub fn getattr(&self, ino: u64) -> VfsAttr {
        match self.nodes.get(&ino) {
            Some(Node::Directory { .. }) => {
                let mut attr = make_attr(ino, 4096, self.uid, self.gid);
                attr.kind = FileType::Directory;
                attr.perm = 0o755;
                attr
            }
            Some(Node::File { size, mode, .. }) => {
                let mut attr = make_attr(ino, *size, self.uid, self.gid);
                attr.kind = FileType::RegularFile;
                attr.perm = (*mode as u16) & 0o777;
                attr
            }
            None => make_attr(ino, 0, self.uid, self.gid), // Fallback
        }
    }

    /// Returns the file type of the given inode, if it exists.
    pub fn node_type(&self, ino: u64) -> Option<FileType> {
        match self.nodes.get(&ino) {
            Some(Node::Directory { .. }) => Some(FileType::Directory),
            Some(Node::File { .. }) => Some(FileType::RegularFile),
            None => None,
        }
    }

    /// Returns the size, mode, and mtime of the given file inode.
    pub fn file_metadata(&self, ino: u64) -> Option<(u64, u32, u64)> {
        match self.nodes.get(&ino) {
            Some(Node::File { size, mode, mtime, .. }) => Some((*size, *mode, *mtime)),
            _ => None,
        }
    }
}
