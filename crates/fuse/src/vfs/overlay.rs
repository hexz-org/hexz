//! Overlay file format and modified-block tracking.
//!
//! Implements the copy-on-write overlay used to record guest writes without
//! mutating the base snapshot. Overlay data and a `.meta` sidecar store
//! block indices; the FUSE adapter merges overlay and base at read time.

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Logical block size used by the overlay file in bytes.
///
/// **Architectural intent:** Aligns overlay writes and modified-block metadata
/// with the FUSE adapter's notion of disk granularity, simplifying block
/// tracking and reducing fragmentation.
///
/// **Constraints:** Must match the block granularity used by the FUSE
/// adapter; changing it requires coordinated updates to `Strata` read/write
/// logic and on-disk overlay metadata.
pub const BLOCK_SIZE: u64 = 4096;

/// Size in bytes of a single modified-block index in the `.meta` file (8).
///
/// **Architectural intent:** Each entry is a little-endian `u64` block index;
/// fixed size allows sequential read of the metadata file without a separate
/// index. Must match the encoding used when appending in `mark_block_modified`.
///
/// **Constraints:** Changing this breaks compatibility with existing `.meta`
/// files; decode and encode paths must stay in sync.
const META_ENTRY_SIZE: usize = 8;

/// On-disk overlay tracking modified blocks of a mounted snapshot.
///
/// **Architectural intent:** Records guest writes in a separate file so that
/// the base snapshot remains immutable while still allowing incremental
/// changes to be preserved across mounts.
///
/// **Constraints:** The overlay and its `.meta` sidecar must be stored on a
/// filesystem that supports regular file I/O; concurrent writers are not
/// synchronized beyond this struct's internal use.
pub struct Overlay {
    pub file: File,
    pub modified_blocks: HashSet<u64>,
    meta_file: File,
}

impl Overlay {
    /// Opens or creates an overlay and its metadata sidecar at `path`.
    ///
    /// **Architectural intent:** Rehydrates the in-memory set of modified
    /// blocks by scanning the `.meta` file so that subsequent reads and
    /// writes can respect prior guest changes.
    ///
    /// **Constraints:** The target path and its `.meta` companion must be
    /// readable and writable; a corrupted `.meta` file will cause this
    /// constructor to fail rather than silently discard overlay history.
    ///
    /// **Side effects:** Touches the filesystem to create or open the data and
    /// metadata files and may iterate over a large `.meta` file for heavily
    /// modified overlays.
    pub fn new(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        let meta_path = path.with_extension("meta");
        let mut meta_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&meta_path)?;

        let mut modified_blocks = HashSet::new();

        let mut buf = [0u8; META_ENTRY_SIZE];
        loop {
            match meta_file.read_exact(&mut buf) {
                Ok(_) => {
                    let block_idx = u64::from_le_bytes(buf);
                    modified_blocks.insert(block_idx);
                }
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }
        }

        Ok(Self {
            file,
            modified_blocks,
            meta_file,
        })
    }

    /// Persists overlay metadata to disk if a multi-step flush is required.
    ///
    /// **Architectural intent:** Reserved for future use; current
    /// implementations rely on incremental updates performed in
    /// `mark_block_modified`.
    ///
    /// **Constraints:** Callers should not depend on this being invoked for
    /// durability; metadata is written eagerly on each new block modification.
    ///
    /// **Side effects:** Currently a no-op, returning success without
    /// performing I/O.
    pub fn save_metadata(&self, _overlay_path: &Path) -> io::Result<()> {
        Ok(())
    }

    /// Returns the current logical length of the overlay file in bytes.
    ///
    /// **Architectural intent:** Allows the FUSE adapter to report file size
    /// and block counts that include overlay growth beyond the base snapshot.
    ///
    /// **Constraints:** Length is derived from filesystem metadata and may lag
    /// slightly behind in-flight writes until they are flushed.
    pub fn len(&self) -> u64 {
        self.file.metadata().map(|m| m.len()).unwrap_or(0)
    }

    /// Returns whether the overlay file has zero length (no overlay data yet).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Indicates whether a particular block index has been modified.
    ///
    /// **Architectural intent:** Provides a constant-time query used by read
    /// paths to decide whether to serve data from the overlay or the base
    /// snapshot.
    ///
    /// **Constraints:** `block_idx` is expressed in units of `BLOCK_SIZE` and
    /// must match the indices used by the reader and writer.
    pub fn is_block_modified(&self, block_idx: u64) -> bool {
        self.modified_blocks.contains(&block_idx)
    }

    /// Marks a block as modified and appends its index to the metadata file.
    ///
    /// **Architectural intent:** Maintains an append-only log of modified
    /// blocks so that overlay state can be reconstructed quickly on the next
    /// mount without scanning the entire data file.
    ///
    /// **Constraints:** Duplicate entries are suppressed in memory but may
    /// still appear in the `.meta` file if crashes occur between writes;
    /// readers must tolerate repeated indices.
    ///
    /// **Side effects:** Writes the block index to the `.meta` file and
    /// flushes it to disk, introducing synchronous I/O on the first write to
    /// each block.
    pub fn mark_block_modified(&mut self, block_idx: u64) {
        if self.modified_blocks.insert(block_idx) {
            let bytes = block_idx.to_le_bytes();
            let _ = self.meta_file.write_all(&bytes);
            let _ = self.meta_file.flush();
        }
    }

    /// Reads raw bytes from the overlay data file into `buf`.
    ///
    /// **Architectural intent:** Provides low-level I/O primitives for the
    /// FUSE adapter without exposing the underlying `File` handle directly.
    ///
    /// **Constraints:** Callers must ensure that `offset + buf.len()` does not
    /// exceed the overlay length; short reads propagate standard `io::Error`s.
    pub fn read_file(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read(buf)
    }

    /// Writes raw bytes from `data` into the overlay data file at `offset`.
    ///
    /// **Architectural intent:** Supports both block-seeded writes and direct
    /// user writes as part of the copy-on-write pipeline.
    ///
    /// **Constraints:** Callers are responsible for calling
    /// `mark_block_modified` for the corresponding blocks; this method only
    /// manipulates file contents.
    ///
    /// **Side effects:** Seeks and writes to the overlay file and may extend
    /// its length, allocating additional space on disk.
    pub fn write_file(&mut self, offset: u64, data: &[u8]) -> io::Result<usize> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write(data)
    }
}
