//! Archive manifest for directory-based archives.
//!
//! Stores the mapping of logical file paths to their byte ranges within
//! the archive's Main stream.

use serde::{Deserialize, Serialize};

/// Metadata for a single file entry in the archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Logical path relative to archive root (e.g. "src/main.rs")
    pub path: String,
    /// Logical offset within the `ArchiveStream::Main`
    pub offset: u64,
    /// File size in bytes
    pub size: u64,
    /// POSIX file mode (permissions)
    pub mode: u32,
    /// Last modification time (Unix timestamp)
    pub mtime: u64,
}

/// A complete manifest of all files stored in the archive.
///
/// Serialized as JSON and stored in the archive's metadata section.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArchiveManifest {
    /// List of file entries, typically sorted by path
    pub files: Vec<FileEntry>,
}

impl ArchiveManifest {
    /// Finds a file entry by its logical path.
    pub fn find_file(&self, path: &str) -> Option<&FileEntry> {
        self.files.iter().find(|f| f.path == path)
    }
}
