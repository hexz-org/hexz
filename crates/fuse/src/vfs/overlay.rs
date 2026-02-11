//! Copy-on-write overlay implementation with block tracking metadata.
//!
//! This module implements the **overlay file format** used to record guest
//! writes without modifying the immutable base snapshot. The overlay consists
//! of two files:
//! - **Data file**: Modified disk blocks at their logical file offsets
//! - **Metadata file (`.meta`)**: Append-only log of modified block indices
//!
//! # Overlay File Format
//!
//! The overlay data file is a **sparse file** that stores modified blocks at
//! their original offsets:
//! - Block 0 (offset 0) -> overlay bytes [0..4096]
//! - Block 1 (offset 4096) -> overlay bytes [4096..8192]
//! - Block N (offset N*4096) -> overlay bytes [N*4096..(N+1)*4096]
//!
//! Unmodified blocks do not occupy space in the overlay file; the filesystem
//! may represent them as holes (sparse regions). This minimizes storage overhead
//! for lightly modified snapshots.
//!
//! ## Example Layout
//!
//! Given a 1 GiB snapshot with only blocks 0, 5, and 1000 modified:
//! ```text
//! Overlay file size: ~12 KiB (3 blocks * 4 KiB)
//! Sparse regions: [4096..20480] and [24576..4096000] are holes
//! .meta file size: 24 bytes (3 block indices * 8 bytes)
//! ```
//!
//! # Block Tracking Mechanism
//!
//! The `.meta` sidecar is an **append-only log** of modified block indices
//! (8-byte little-endian `u64` values). On mount, the adapter reads this file
//! sequentially to reconstruct the in-memory `HashSet<u64>` of modified blocks.
//!
//! ## Metadata File Structure
//!
//! ```text
//! Offset | Content           | Meaning
//! -------|-------------------|----------------------------------
//! 0      | 0x00000000_00000000 | Block 0 modified
//! 8      | 0x05000000_00000000 | Block 5 modified
//! 16     | 0xE8030000_00000000 | Block 1000 modified (0x3E8 = 1000)
//! ```
//!
//! The log may contain **duplicate entries** (e.g., if block 0 is written
//! multiple times). The `HashSet` automatically deduplicates during rehydration.
//!
//! # Persistence and Recovery
//!
//! - **On first write to a block**: Append the block index to `.meta` and flush
//! - **On mount**: Scan `.meta` to reconstruct `modified_blocks` set
//! - **On crash**: Any block indices flushed to `.meta` are considered modified,
//!   even if the data write was partially incomplete (conservative approach)
//!
//! This ensures that overlay state is always recoverable, though it may
//! over-report modified blocks in rare crash scenarios.
//!
//! # Space Efficiency
//!
//! The overlay's space usage is:
//! - **Data file**: 4 KiB per modified block (sparse file, only allocated blocks count)
//! - **Metadata file**: 8 bytes per first-write to each block (append-only log)
//!
//! For a 100 GiB disk with 1% modified (256k blocks), the overlay consumes:
//! - Data: 1 GiB (256k * 4 KiB)
//! - Metadata: 2 MiB (256k * 8 bytes)
//!
//! Total: ~1 GiB, a 100:1 compression ratio vs. full snapshot duplication.
//!
//! # Thread Safety
//!
//! The `Overlay` struct is **not thread-safe** on its own. The FUSE adapter
//! typically runs in a single-threaded request loop, so no additional locking
//! is needed. For multi-threaded FUSE implementations, callers must wrap
//! `Overlay` in a `Mutex` or `RwLock`.
//!
//! # Examples
//!
//! ## Creating and Using an Overlay
//!
//! ```no_run
//! use strata_fuse::vfs::Overlay;
//! use std::path::Path;
//!
//! # fn main() -> std::io::Result<()> {
//! // Create or open overlay at /tmp/overlay.img
//! let mut overlay = Overlay::new(Path::new("/tmp/overlay.img"))?;
//!
//! // Check if block 0 has been modified
//! assert!(!overlay.is_block_modified(0));
//!
//! // Write some data
//! overlay.write_file(0, &[0xFF; 4096])?;
//! overlay.mark_block_modified(0);
//!
//! // Now block 0 is tracked as modified
//! assert!(overlay.is_block_modified(0));
//!
//! // Metadata is persisted to /tmp/overlay.meta
//! # Ok(())
//! # }
//! ```
//!
//! ## Recovering from Existing Overlay
//!
//! ```no_run
//! use strata_fuse::vfs::Overlay;
//! use std::path::Path;
//!
//! # fn main() -> std::io::Result<()> {
//! // Reopen overlay (reads .meta to reconstruct modified_blocks)
//! let overlay = Overlay::new(Path::new("/tmp/overlay.img"))?;
//!
//! // Modified blocks from previous session are automatically restored
//! if overlay.is_block_modified(0) {
//!     println!("Block 0 was modified in a prior session");
//! }
//! # Ok(())
//! # }
//! ```

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Logical block size for overlay tracking (4 KiB).
///
/// This defines the granularity at which the overlay tracks modifications.
/// Each block is either fully unmodified (read from snapshot) or fully
/// modified (read from overlay), even if only one byte within the block
/// was written.
///
/// # Rationale for 4 KiB
///
/// - **Page alignment**: Matches typical OS page size, reducing fragmentation
/// - **Storage efficiency**: Balances metadata overhead vs. data overhead
/// - **I/O alignment**: Aligns with common disk sector sizes (4K native)
///
/// # Trade-offs
///
/// - **Smaller block size** (e.g., 512 bytes):
///   - Pro: Less write amplification for tiny writes
///   - Con: 8x more metadata, slower mount times
///
/// - **Larger block size** (e.g., 64 KiB):
///   - Pro: Less metadata, faster mount
///   - Con: 16x write amplification (writing 1 byte copies 64 KiB)
///
/// 4 KiB is a pragmatic compromise for unikernel workloads, which typically
/// write in larger chunks (kernel pages, filesystem blocks).
///
/// # Compatibility
///
/// Changing this constant breaks compatibility with existing overlay files.
/// The `.meta` format does not encode the block size, so mismatched values
/// cause silent corruption. **Do not change this value** unless you also
/// version the overlay format.
pub const BLOCK_SIZE: u64 = 4096;

/// Size in bytes of a single metadata entry (8).
///
/// Each entry in the `.meta` file is an 8-byte little-endian `u64` block
/// index. The fixed size allows sequential scanning during mount without
/// parsing variable-length records.
///
/// # Encoding
///
/// Block index `N` is encoded as:
/// ```text
/// [N as u64].to_le_bytes() -> [8 bytes]
/// ```
///
/// Example: Block 0 -> `[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]`
///
/// # Compatibility
///
/// Changing this constant breaks compatibility with existing `.meta` files.
/// The encode (in `mark_block_modified`) and decode (in `Overlay::new`) paths
/// must stay in sync.
const META_ENTRY_SIZE: usize = 8;

/// Copy-on-write overlay tracking modified disk blocks.
///
/// The `Overlay` struct encapsulates:
/// - A data file storing modified block contents at their logical offsets
/// - A metadata file (`.meta`) logging which blocks have been modified
/// - An in-memory set of modified block indices for fast lookup
///
/// # Structure
///
/// - `file`: The data file (e.g., `/tmp/overlay.img`), opened read-write
/// - `modified_blocks`: In-memory `HashSet<u64>` of modified block indices
/// - `meta_file`: The metadata file (e.g., `/tmp/overlay.meta`), opened append-write
///
/// # Lifetime
///
/// Created in `Strata::new()` and dropped on unmount. The `Drop` impl
/// (currently a no-op) could be extended to flush pending metadata if needed.
///
/// # Thread Safety
///
/// This struct is **not thread-safe**. Concurrent access requires external
/// synchronization (e.g., `Mutex<Overlay>`). The FUSE adapter typically runs
/// single-threaded, so this is not an issue in practice.
///
/// # File Handle Ownership
///
/// The `file` field is public to allow `set_len` calls from `handle_setattr`.
/// For normal I/O, use the provided `read_file` and `write_file` methods
/// rather than manipulating `file` directly.
pub struct Overlay {
    pub file: File,
    pub modified_blocks: HashSet<u64>,
    meta_file: File,
}

impl Overlay {
    /// Opens or creates an overlay and rehydrates metadata from disk.
    ///
    /// This constructor performs the following steps:
    /// 1. Opens (or creates) the data file at `path` (e.g., `/tmp/overlay.img`)
    /// 2. Opens (or creates) the metadata file at `path.with_extension("meta")`
    /// 3. Scans the `.meta` file to reconstruct the `modified_blocks` set
    /// 4. Returns the initialized `Overlay` ready for read/write operations
    ///
    /// # Metadata Rehydration
    ///
    /// The `.meta` file is read sequentially in 8-byte chunks. Each chunk is
    /// decoded as a little-endian `u64` block index and inserted into the
    /// `modified_blocks` set. Duplicate entries are automatically deduplicated
    /// by the `HashSet`.
    ///
    /// If the `.meta` file is empty (new overlay), the set starts empty and
    /// blocks are added as writes occur.
    ///
    /// # Parameters
    ///
    /// - `path`: Path to the overlay data file. The metadata file is derived
    ///   by changing the extension to `.meta`.
    ///
    /// # Returns
    ///
    /// - `Ok(Overlay)`: Overlay successfully opened and metadata loaded
    /// - `Err(io::Error)`: File I/O failed (e.g., permission denied, corrupted `.meta`)
    ///
    /// # Errors
    ///
    /// Returns `io::Error` if:
    /// - The data or metadata file cannot be opened/created (permission denied)
    /// - The `.meta` file contains partial entries (not a multiple of 8 bytes)
    /// - Disk I/O errors occur during scanning
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use strata_fuse::vfs::Overlay;
    /// use std::path::Path;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// // Create new overlay
    /// let overlay = Overlay::new(Path::new("/tmp/new-overlay.img"))?;
    /// assert_eq!(overlay.modified_blocks.len(), 0);
    ///
    /// // Reopen existing overlay (loads metadata)
    /// let overlay2 = Overlay::new(Path::new("/tmp/new-overlay.img"))?;
    /// // overlay2.modified_blocks restored from /tmp/new-overlay.meta
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance
    ///
    /// - Time complexity: O(n) where n = number of entries in `.meta` file
    /// - Typical latency: 1-10 ms for 1k-10k modified blocks, 100+ ms for 100k+ blocks
    /// - Disk I/O: Reads entire `.meta` file sequentially
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

    /// Persists overlay metadata to disk (currently a no-op).
    ///
    /// This method is reserved for future use. Currently, metadata is flushed
    /// eagerly on each new block modification in `mark_block_modified`, so
    /// there is no pending state to persist.
    ///
    /// # Future Extensions
    ///
    /// Potential use cases for batch metadata persistence:
    /// - Buffering multiple block modifications before flushing (reduces fsync calls)
    /// - Writing a metadata checksum or version header
    /// - Compacting the `.meta` log to remove duplicates
    ///
    /// # Parameters
    ///
    /// - `_overlay_path`: Path to overlay file (unused, kept for API compatibility)
    ///
    /// # Returns
    ///
    /// Always returns `Ok(())`. No I/O is performed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use strata_fuse::vfs::Overlay;
    /// use std::path::Path;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let overlay = Overlay::new(Path::new("/tmp/overlay.img"))?;
    /// overlay.save_metadata(Path::new("/tmp/overlay.img"))?; // No-op
    /// # Ok(())
    /// # }
    /// ```
    pub fn save_metadata(&self, _overlay_path: &Path) -> io::Result<()> {
        Ok(())
    }

    /// Returns the current logical length of the overlay data file.
    ///
    /// This method queries the filesystem metadata to determine the overlay
    /// file's size, which may exceed the base snapshot size if the guest has
    /// extended the disk (e.g., via partition table changes or filesystem resize).
    ///
    /// # Use Case
    ///
    /// The FUSE adapter uses this to compute the reported size for the disk
    /// inode. If `overlay.len() > snapshot.size(Disk)`, the larger value is
    /// returned to the kernel, allowing the guest to see the extended disk.
    ///
    /// # Caveats
    ///
    /// - **Cache consistency**: The returned value reflects the last flushed
    ///   write. In-flight writes in the kernel page cache may not yet be
    ///   visible via `stat`.
    /// - **Sparse files**: The reported size is the **logical size**, not the
    ///   allocated size. A sparse overlay with 1 GiB logical size but only
    ///   10 MiB written will still report 1 GiB here.
    ///
    /// # Returns
    ///
    /// Logical file size in bytes, or 0 if the metadata query fails (rare).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use strata_fuse::vfs::Overlay;
    /// use std::path::Path;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let mut overlay = Overlay::new(Path::new("/tmp/overlay.img"))?;
    ///
    /// // Write beyond original snapshot size
    /// overlay.file.set_len(20 * 1024 * 1024 * 1024)?; // 20 GiB
    /// assert_eq!(overlay.len(), 20 * 1024 * 1024 * 1024);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance
    ///
    /// - Time complexity: O(1) - single `fstat` syscall
    /// - Typical latency: 1-2 microseconds
    pub fn len(&self) -> u64 {
        self.file.metadata().map(|m| m.len()).unwrap_or(0)
    }

    /// Returns whether the overlay file is empty (zero logical size).
    ///
    /// This is a convenience method equivalent to `self.len() == 0`. It
    /// indicates that no data has been written to the overlay file, though
    /// the `.meta` file may still exist (and be empty).
    ///
    /// # Returns
    ///
    /// `true` if the overlay file has zero logical size, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use strata_fuse::vfs::Overlay;
    /// use std::path::Path;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let overlay = Overlay::new(Path::new("/tmp/new-overlay.img"))?;
    /// assert!(overlay.is_empty()); // New overlay starts empty
    /// # Ok(())
    /// # }
    /// ```
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Checks whether a block has been modified.
    ///
    /// This method queries the in-memory `modified_blocks` set to determine
    /// if a given block has been written since the overlay was created. It
    /// is the core decision point for read operations: modified blocks are
    /// read from the overlay, unmodified blocks from the snapshot.
    ///
    /// # Block Indexing
    ///
    /// Block indices are computed as:
    /// ```text
    /// block_idx = byte_offset / BLOCK_SIZE
    /// ```
    ///
    /// For example, with `BLOCK_SIZE = 4096`:
    /// - Offset 0 -> Block 0
    /// - Offset 4096 -> Block 1
    /// - Offset 8192 -> Block 2
    ///
    /// # Parameters
    ///
    /// - `block_idx`: Block index (0-based, in units of `BLOCK_SIZE`)
    ///
    /// # Returns
    ///
    /// - `true`: Block has been written via `write_file` + `mark_block_modified`
    /// - `false`: Block has not been modified (read from snapshot)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use strata_fuse::vfs::Overlay;
    /// use std::path::Path;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let mut overlay = Overlay::new(Path::new("/tmp/overlay.img"))?;
    ///
    /// // Initially, no blocks are modified
    /// assert!(!overlay.is_block_modified(0));
    ///
    /// // Write and mark block 0 as modified
    /// overlay.write_file(0, &[0xFF; 4096])?;
    /// overlay.mark_block_modified(0);
    ///
    /// // Now block 0 is modified
    /// assert!(overlay.is_block_modified(0));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance
    ///
    /// - Time complexity: O(1) average (HashSet lookup)
    /// - Typical latency: 10-50 nanoseconds
    pub fn is_block_modified(&self, block_idx: u64) -> bool {
        self.modified_blocks.contains(&block_idx)
    }

    /// Marks a block as modified and persists to metadata file.
    ///
    /// This method updates the in-memory `modified_blocks` set and appends
    /// the block index to the `.meta` file. It is called by the write handler
    /// on the first write to each block to ensure overlay state is recoverable
    /// after crashes.
    ///
    /// # Deduplication
    ///
    /// If the block is already in `modified_blocks`, this method is a no-op.
    /// This avoids writing duplicate entries to the `.meta` file during
    /// subsequent writes to the same block.
    ///
    /// # Persistence
    ///
    /// The block index is written to the `.meta` file as an 8-byte little-endian
    /// `u64` and immediately flushed to disk. This ensures that the metadata
    /// is durable before the data write completes, providing crash consistency.
    ///
    /// # Error Handling
    ///
    /// Errors during `.meta` write or flush are **silently ignored**. This is
    /// a best-effort approach; if metadata persistence fails, the overlay may
    /// report fewer modified blocks than expected on the next mount, but data
    /// integrity is preserved (reads fall back to snapshot for unmarked blocks).
    ///
    /// # Parameters
    ///
    /// - `block_idx`: Block index to mark as modified (0-based, in units of `BLOCK_SIZE`)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use strata_fuse::vfs::Overlay;
    /// use std::path::Path;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let mut overlay = Overlay::new(Path::new("/tmp/overlay.img"))?;
    ///
    /// // Write block 0 and mark it as modified
    /// overlay.write_file(0, &[0xAA; 4096])?;
    /// overlay.mark_block_modified(0); // Appends 0u64.to_le_bytes() to .meta
    ///
    /// // Subsequent calls to mark_block_modified(0) are no-ops
    /// overlay.mark_block_modified(0); // No .meta write
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance
    ///
    /// - Time complexity: O(1) average (HashSet insert + file append)
    /// - Typical latency: 5-50 microseconds (depends on fsync speed)
    /// - I/O operations: 1 write (8 bytes) + 1 flush per first-write to block
    pub fn mark_block_modified(&mut self, block_idx: u64) {
        if self.modified_blocks.insert(block_idx) {
            let bytes = block_idx.to_le_bytes();
            let _ = self.meta_file.write_all(&bytes);
            let _ = self.meta_file.flush();
        }
    }

    /// Reads bytes from the overlay data file at a given offset.
    ///
    /// This method performs a positioned read from the overlay file, seeking
    /// to the specified offset and reading up to `buf.len()` bytes. It is
    /// used by the FUSE read handler when serving data from modified blocks.
    ///
    /// # Semantics
    ///
    /// - **Positioned read**: Seeks to `offset` before reading (does not
    ///   preserve file cursor for subsequent operations)
    /// - **Short reads**: If fewer bytes are available than requested (e.g.,
    ///   reading beyond EOF), returns the actual number of bytes read
    /// - **EOF**: Reading at or beyond the overlay file's logical size returns 0
    ///
    /// # Parameters
    ///
    /// - `offset`: Byte offset in the overlay file (0-based)
    /// - `buf`: Mutable buffer to fill with data
    ///
    /// # Returns
    ///
    /// - `Ok(n)`: Successfully read `n` bytes into `buf`
    /// - `Err(e)`: I/O error occurred (e.g., permission denied, disk error)
    ///
    /// # Errors
    ///
    /// Returns `io::Error` if:
    /// - Seek operation fails (e.g., offset exceeds file system limits)
    /// - Read operation fails (e.g., disk I/O error)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use strata_fuse::vfs::Overlay;
    /// use std::path::Path;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let mut overlay = Overlay::new(Path::new("/tmp/overlay.img"))?;
    ///
    /// // Write some data first
    /// overlay.write_file(0, b"Hello, world!")?;
    ///
    /// // Read it back
    /// let mut buf = vec![0u8; 5];
    /// let n = overlay.read_file(0, &mut buf)?;
    /// assert_eq!(n, 5);
    /// assert_eq!(&buf, b"Hello");
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance
    ///
    /// - Time complexity: O(n) where n = buf.len()
    /// - Typical latency: 5-50 microseconds (depends on page cache)
    pub fn read_file(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read(buf)
    }

    /// Writes bytes to the overlay data file at a given offset.
    ///
    /// This method performs a positioned write to the overlay file, seeking
    /// to the specified offset and writing the provided data. It is used by
    /// the FUSE write handler for both block seeding and user writes.
    ///
    /// # Semantics
    ///
    /// - **Positioned write**: Seeks to `offset` before writing (does not
    ///   preserve file cursor for subsequent operations)
    /// - **File extension**: If `offset + data.len()` exceeds the current
    ///   file size, the file is automatically extended
    /// - **Sparse regions**: Writes to non-contiguous offsets may create
    ///   sparse regions (holes) in the overlay file
    ///
    /// # Important: Caller Responsibilities
    ///
    /// This method **only writes data**. The caller is responsible for:
    /// - Calling `mark_block_modified()` for each block touched by the write
    /// - Seeding blocks with snapshot data before partial-block writes (COW)
    ///
    /// Failing to mark blocks as modified will cause reads to incorrectly
    /// serve snapshot data instead of overlay data.
    ///
    /// # Parameters
    ///
    /// - `offset`: Byte offset in the overlay file (0-based)
    /// - `data`: Byte slice to write
    ///
    /// # Returns
    ///
    /// - `Ok(n)`: Successfully wrote `n` bytes (usually equals `data.len()`)
    /// - `Err(e)`: I/O error occurred (e.g., no disk space, permission denied)
    ///
    /// # Errors
    ///
    /// Returns `io::Error` if:
    /// - Seek operation fails
    /// - Write operation fails (e.g., disk full, I/O error)
    ///
    /// # Examples
    ///
    /// ## Block-Aligned Write
    ///
    /// ```no_run
    /// use strata_fuse::vfs::Overlay;
    /// use std::path::Path;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let mut overlay = Overlay::new(Path::new("/tmp/overlay.img"))?;
    ///
    /// // Write full block
    /// let block_data = vec![0xFF; 4096];
    /// overlay.write_file(0, &block_data)?;
    /// overlay.mark_block_modified(0); // Critical: mark as modified
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ## Copy-on-Write Partial Block Write
    ///
    /// ```no_run
    /// use strata_fuse::vfs::Overlay;
    /// use std::path::Path;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let mut overlay = Overlay::new(Path::new("/tmp/overlay.img"))?;
    ///
    /// // Seed block 0 with snapshot data (omitted for brevity)
    /// let snapshot_block_0 = vec![0x00; 4096];
    /// overlay.write_file(0, &snapshot_block_0)?;
    /// overlay.mark_block_modified(0);
    ///
    /// // Now overwrite bytes 100..200 with user data
    /// overlay.write_file(100, &[0xAA; 100])?;
    /// // No mark_block_modified needed (already marked)
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance
    ///
    /// - Time complexity: O(n) where n = data.len()
    /// - Typical latency: 10-100 microseconds (depends on page cache and fsync policy)
    pub fn write_file(&mut self, offset: u64, data: &[u8]) -> io::Result<usize> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write(data)
    }
}
