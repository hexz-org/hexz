//! Virtual filesystem abstractions for FUSE.
//!
//! This module contains the core VFS logic for mapping Strata snapshots
//! to FUSE inodes, managing file attributes, and handling overlay state.

pub mod attr;
pub mod inode;
pub mod overlay;

pub use inode::{DirEntry, Inode, InodeMap, InodeType};
pub use overlay::{BLOCK_SIZE, Overlay};
