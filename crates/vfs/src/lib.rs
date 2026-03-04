//! Platform-agnostic virtual filesystem logic for Hexz.
//!
//! This crate will house the core inode mapping, attribute synthesis,
//! and directory layout logic that is shared between FUSE, NFS, and WebDAV.

pub mod vfs {
    pub struct InodeMapStub;
}
