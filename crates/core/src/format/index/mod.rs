//! Snapshot index structures for mapping logical offsets to physical blocks.
//!
//! # Overview
//!
//! Strata snapshots use a two-level index hierarchy:
//! 1. **Master Index**: Top-level directory of index pages (stored at end of file)
//! 2. **Index Pages**: Arrays of `BlockInfo` records for contiguous block ranges
//!
//! This design enables:
//! - Fast random access (binary search master index → read single page)
//! - Efficient streaming (sequential page reads)
//! - Lazy loading (only load pages needed for requested ranges)
//!
//! # Index Layout
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │ Header (512B)                                       │
//! ├─────────────────────────────────────────────────────┤
//! │ Compressed Block 0                                  │
//! │ Compressed Block 1                                  │
//! │ ...                                                 │
//! │ Compressed Block N                                  │
//! ├─────────────────────────────────────────────────────┤
//! │ Index Page 0 (bincode-serialized BlockInfo[])      │
//! │ Index Page 1                                        │
//! │ ...                                                 │
//! ├─────────────────────────────────────────────────────┤
//! │ Master Index (bincode-serialized PageEntry[])      │ ← header.index_offset
//! └─────────────────────────────────────────────────────┘
//! ```
//!
//! # Random Access Workflow
//!
//! To read data at logical offset `O`:
//! 1. Binary search `master.disk_pages` for page covering `O`
//! 2. Read and deserialize the index page
//! 3. Find block(s) overlapping `O`
//! 4. Read compressed block from `BlockInfo.offset`
//! 5. Decompress and extract relevant bytes
//!
//! # Performance
//!
//! - **Cold read**: ~1ms (2 seeks + decompress)
//! - **Warm read**: ~80μs (cached index + block)
//! - **Sequential read**: ~2-3 GB/s (prefetch + streaming decompression)
//!
//! # Examples
//!
//! See [`crate::api::stratafile::StrataFile`] for usage examples.

use serde::{Deserialize, Serialize};

/// B-Tree index for efficient range queries (alternative indexing strategy).
pub mod btree;

/// Hash-based index for fast random access and deduplication (alternative strategy).
pub mod hash;

/// Maximum number of `BlockInfo` entries per index page.
///
/// This constant balances:
/// - **Memory usage**: Each page ~64KB serialized (4096 * 16 bytes)
/// - **Granularity**: Finer pages → faster partial reads, but more overhead
/// - **Cache efficiency**: Pages fit in L2/L3 cache
///
/// With 4KB blocks, each page covers ~16MB of logical data.
pub const ENTRIES_PER_PAGE: usize = 4096;

/// Metadata for a single compressed block in the snapshot.
///
/// Each block represents a contiguous chunk of logical data (typically 4KB-64KB)
/// that has been compressed, optionally encrypted, and written to the snapshot file.
///
/// # Fields
///
/// - **offset**: Physical byte offset in the snapshot file (where compressed data starts)
/// - **length**: Compressed size in bytes (0 for sparse/zero blocks)
/// - **logical_len**: Uncompressed size in bytes (original data size)
/// - **checksum**: CRC32 of compressed data (for integrity verification)
///
/// # Special Values
///
/// - `offset = BLOCK_OFFSET_PARENT` (u64::MAX): Block stored in parent snapshot (thin snapshots)
/// - `length = 0`: Sparse block (all zeros, not stored on disk)
///
/// # Size
///
/// This struct is 20 bytes, kept compact to minimize index overhead.
///
/// # Examples
///
/// ```
/// use strata_core::format::index::BlockInfo;
///
/// // Normal block
/// let block = BlockInfo {
///     offset: 4096,         // Starts at byte 4096
///     length: 2048,         // Compressed to 2KB
///     logical_len: 4096,    // Original 4KB
///     checksum: 0x12345678,
/// };
///
/// // Sparse (zero) block
/// let sparse = BlockInfo {
///     offset: 0,
///     length: 0,           // Not stored
///     logical_len: 4096,   // But logically 4KB
///     checksum: 0,
/// };
/// ```
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct BlockInfo {
    /// Physical offset in the snapshot file (bytes).
    pub offset: u64,

    /// Compressed size in bytes (0 for sparse blocks).
    pub length: u32,

    /// Uncompressed logical size in bytes.
    pub logical_len: u32,

    /// CRC32 checksum of compressed data.
    pub checksum: u32,
}

/// Master index entry pointing to a serialized index page.
///
/// Each `PageEntry` describes the location of an index page containing up to
/// `ENTRIES_PER_PAGE` block metadata records. The master index is an array
/// of these entries, stored at the end of the snapshot file.
///
/// # Fields
///
/// - **offset**: Physical byte offset of the serialized index page
/// - **length**: Size of the serialized page in bytes
/// - **start_block**: Global block index of the first block in this page
/// - **start_logical**: Logical byte offset where this page's coverage begins
///
/// # Usage
///
/// To find the page covering logical offset `O`:
/// ```text
/// binary_search(master.disk_pages, |p| p.start_logical.cmp(&O))
/// ```
///
/// # Serialization
///
/// Pages are serialized using `bincode` and stored contiguously before the
/// master index. The page entry provides the offset and length for deserialization.
///
/// # Examples
///
/// ```
/// use strata_core::format::index::PageEntry;
///
/// let entry = PageEntry {
///     offset: 1048576,      // Page starts at 1MB
///     length: 65536,        // Page is 64KB serialized
///     start_block: 0,       // First block is block #0
///     start_logical: 0,     // Covers logical bytes 0..N
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageEntry {
    /// Physical offset of the index page in the snapshot file.
    pub offset: u64,

    /// Serialized size of the index page in bytes.
    pub length: u32,

    /// Global block index of the first block in this page.
    pub start_block: u64,

    /// Logical byte offset where this page's coverage begins.
    pub start_logical: u64,
}

/// Top-level index stored at the end of a snapshot file.
///
/// The master index is the entry point for all random access operations. It
/// contains separate page directories for disk and memory streams, plus logical
/// size metadata for each stream.
///
/// # Structure
///
/// - **disk_pages**: Index entries for the disk stream (persistent storage)
/// - **memory_pages**: Index entries for the memory stream (volatile state)
/// - **disk_size**: Total logical size of disk stream (uncompressed bytes)
/// - **memory_size**: Total logical size of memory stream (uncompressed bytes)
///
/// # Location
///
/// The master index is always stored at the end of the snapshot file. Its offset
/// is recorded in the snapshot header (`header.index_offset`).
///
/// # Serialization
///
/// Serialized using `bincode`. Typical size: ~1KB per 1GB of data (with 64KB pages).
///
/// # Random Access Algorithm
///
/// ```text
/// To read from disk stream at offset O:
/// 1. page_idx = binary_search(master.disk_pages, |p| p.start_logical.cmp(&O))
/// 2. page = read_and_deserialize(page_entry[page_idx])
/// 3. block_info = find_block_in_page(page, O)
/// 4. compressed = backend.read_exact(block_info.offset, block_info.length)
/// 5. data = decompress(compressed)
/// 6. return extract_range(data, O, len)
/// ```
///
/// # Dual Streams
///
/// Disk and memory streams are independently indexed. This enables:
/// - VM snapshots (disk = disk image, memory = RAM dump)
/// - Application snapshots (disk = state, memory = heap)
/// - Separate compression tuning per stream
///
/// # Examples
///
/// ```
/// use strata_core::format::index::{MasterIndex, PageEntry};
///
/// let master = MasterIndex {
///     disk_pages: vec![
///         PageEntry {
///             offset: 4096,
///             length: 65536,
///             start_block: 0,
///             start_logical: 0,
///         }
///     ],
///     memory_pages: vec![],
///     disk_size: 1_000_000_000,  // 1GB logical
///     memory_size: 0,
/// };
///
/// println!("Disk stream: {} GB", master.disk_size / (1024 * 1024 * 1024));
/// println!("Index pages: {}", master.disk_pages.len());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MasterIndex {
    /// Index pages for the disk stream.
    pub disk_pages: Vec<PageEntry>,

    /// Index pages for the memory stream.
    pub memory_pages: Vec<PageEntry>,

    /// Total logical size of the disk stream (uncompressed bytes).
    pub disk_size: u64,

    /// Total logical size of the memory stream (uncompressed bytes).
    pub memory_size: u64,
}

/// Serialized array of block metadata records.
///
/// An index page contains up to `ENTRIES_PER_PAGE` (4096) block metadata entries
/// for a contiguous range of logical blocks. Pages are serialized with `bincode`
/// and stored in the snapshot file before the master index.
///
/// # Size
///
/// - **In-memory**: `Vec<BlockInfo>` (~20 bytes per entry)
/// - **Serialized**: ~64KB for full page (4096 * 16 bytes)
///
/// # Coverage
///
/// With 4KB logical blocks, each page covers:
/// - **Logical data**: ~16MB (4096 blocks * 4KB)
/// - **Physical data**: Depends on compression ratio
///
/// # Access Pattern
///
/// Pages are loaded on-demand when a read operation requires block metadata:
/// 1. Master index binary search identifies page
/// 2. Page is read from disk and deserialized
/// 3. Page is cached in memory (LRU)
/// 4. Block metadata is extracted from page
///
/// # Examples
///
/// ```
/// use strata_core::format::index::{IndexPage, BlockInfo};
///
/// let mut page = IndexPage {
///     blocks: vec![
///         BlockInfo {
///             offset: 4096,
///             length: 2048,
///             logical_len: 4096,
///             checksum: 0x12345678,
///         },
///         BlockInfo {
///             offset: 6144,
///             length: 1024,
///             logical_len: 4096,
///             checksum: 0x9ABCDEF0,
///         },
///     ],
/// };
///
/// // Serialize for storage
/// let bytes = bincode::serialize(&page).unwrap();
/// println!("Page size: {} bytes", bytes.len());
///
/// // Deserialize on read
/// let loaded: IndexPage = bincode::deserialize(&bytes).unwrap();
/// assert_eq!(loaded.blocks.len(), 2);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IndexPage {
    /// Block metadata entries for this page's range.
    pub blocks: Vec<BlockInfo>,
}
