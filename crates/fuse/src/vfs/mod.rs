//! Virtual Filesystem abstractions for FUSE.

pub mod attr;
pub mod inode;

pub use inode::{DirEntry, Inode, InodeMap, ROOT_INODE};
