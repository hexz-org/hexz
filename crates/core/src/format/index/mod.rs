//! Snapshot index structures for mapping logical offsets to physical blocks.
//!
//! # Overview
//!
//! Hexz snapshots use a two-level index hierarchy:
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
//! See [`crate::api::file::File`] for usage examples.

use serde::{Deserialize, Serialize};

/// B-Tree index for efficient range queries (alternative indexing strategy).
pub mod btree;

/// Hash-based index for fast random access and deduplication (alternative strategy).
pub mod hash;

/// Maximum number of `BlockInfo` entries per index page.
///
/// This constant defines the capacity of each index page and is a critical
/// tuning parameter that affects performance, memory usage, and I/O efficiency.
///
/// # Design Tradeoffs
///
/// ## Memory Usage
///
/// Each page contains up to 4096 [`BlockInfo`] entries:
/// - **In-memory size**: ~81,920 bytes (4096 entries * 20 bytes per entry)
/// - **Serialized size**: ~65,536 bytes (bincode compression of repeated zeros)
/// - **Cache footprint**: Fits comfortably in L3 cache (typically 8-16 MB)
///
/// ## Granularity
///
/// With 4KB logical blocks, each page covers:
/// - **Logical data**: ~16 MB (4096 blocks * 4096 bytes)
/// - **Physical reads**: Random access requires loading only the page containing
///   the target block, not the entire index
///
/// Finer granularity (smaller pages) reduces wasted I/O for small reads but
/// increases master index size and binary search overhead.
///
/// ## I/O Efficiency
///
/// Page size optimizes for typical access patterns:
/// - **Random reads**: Single page load (64 KB) + single block read
/// - **Sequential reads**: Stream pages in order, prefetch next page
/// - **Sparse reads**: Skip pages for unused regions (e.g., zero blocks)
///
/// ## Master Index Size
///
/// With 4096 entries per page:
/// - **1 GB snapshot**: ~64 pages (~4 KB master index)
/// - **1 TB snapshot**: ~64,000 pages (~4 MB master index)
///
/// Larger `ENTRIES_PER_PAGE` reduces master index size but increases page load
/// latency for random access.
///
/// # Performance Characteristics
///
/// ## Random Access
///
/// To read a single 4KB block:
/// 1. Binary search master index: O(log P) where P = page count (~10 comparisons for 1 TB)
/// 2. Read index page: ~100 μs (SSD), ~5 ms (HDD)
/// 3. Deserialize page: ~50 μs (bincode deserialize 64 KB)
/// 4. Find block in page: O(1) (direct array indexing)
/// 5. Read block: ~100 μs (SSD), ~5 ms (HDD)
///
/// **Total latency**: ~250 μs (SSD), ~10 ms (HDD) for cold read.
///
/// ## Sequential Access
///
/// Streaming reads benefit from page caching:
/// 1. Load page: ~100 μs (once per 16 MB)
/// 2. Read blocks: ~100 μs * 4096 = ~400 ms (no page reload overhead)
///
/// **Throughput**: ~40 MB/s for page metadata, ~2-3 GB/s for decompressed data.
///
/// # Alternative Values
///
/// | Value | Page Size | Coverage | Use Case |
/// |-------|-----------|----------|----------|
/// | 1024  | ~20 KB    | 4 MB     | Fine-grained random access, small snapshots |
/// | 4096  | ~64 KB    | 16 MB    | **Balanced (current default)** |
/// | 16384 | ~256 KB   | 64 MB    | Sequential access, large snapshots |
///
/// # Examples
///
/// ```
/// use hexz_core::format::index::ENTRIES_PER_PAGE;
///
/// // Calculate how many pages are needed for a 1 GB disk image
/// let block_size = 4096;
/// let disk_size = 1_000_000_000u64;
/// let block_count = (disk_size + block_size - 1) / block_size;
/// let page_count = (block_count as usize + ENTRIES_PER_PAGE - 1) / ENTRIES_PER_PAGE;
///
/// println!("Blocks: {}", block_count);
/// println!("Pages: {}", page_count);
/// println!("Master index size: ~{} KB", page_count * 64 / 1024);
/// // Output: Blocks: 244141, Pages: 60, Master index size: ~3 KB
/// ```
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
/// use hexz_core::format::index::BlockInfo;
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

impl BlockInfo {
    /// Creates a sparse (zero-filled) block descriptor.
    ///
    /// Sparse blocks represent regions of all-zero data that are not physically
    /// stored in the snapshot file. This optimization significantly reduces snapshot
    /// size for sparse disk images (e.g., freshly created filesystems, swap areas).
    ///
    /// # Returns
    ///
    /// A `BlockInfo` with:
    /// - `offset = 0` (not stored on disk)
    /// - `length = 0` (no compressed data)
    /// - `logical_len = len` (represents `len` bytes of zeros)
    /// - `checksum = 0` (no data to checksum)
    ///
    /// # Parameters
    ///
    /// - `len`: Logical size of the zero-filled region in bytes
    ///
    /// # Examples
    ///
    /// ```
    /// use hexz_core::format::index::BlockInfo;
    ///
    /// // Create a sparse 4KB block
    /// let sparse = BlockInfo::sparse(4096);
    /// assert_eq!(sparse.offset, 0);
    /// assert_eq!(sparse.length, 0);
    /// assert_eq!(sparse.logical_len, 4096);
    ///
    /// // When reading this block, reader fills output buffer with zeros
    /// // without performing any I/O.
    /// ```
    pub fn sparse(len: u32) -> Self {
        Self {
            offset: 0,
            length: 0,
            logical_len: len,
            checksum: 0,
        }
    }

    /// Tests whether this block is sparse (all zeros, not stored on disk).
    ///
    /// # Returns
    ///
    /// `true` if `length == 0` and `offset != BLOCK_OFFSET_PARENT`, indicating
    /// that this block is not stored in the snapshot file and should be read as zeros.
    ///
    /// # Examples
    ///
    /// ```
    /// use hexz_core::format::index::BlockInfo;
    ///
    /// let sparse = BlockInfo::sparse(4096);
    /// assert!(sparse.is_sparse());
    ///
    /// let normal = BlockInfo {
    ///     offset: 4096,
    ///     length: 2048,
    ///     logical_len: 4096,
    ///     checksum: 0x12345678,
    /// };
    /// assert!(!normal.is_sparse());
    /// ```
    pub fn is_sparse(&self) -> bool {
        self.length == 0 && self.offset != u64::MAX
    }

    /// Tests whether this block is stored in the parent snapshot.
    ///
    /// For thin snapshots, blocks that haven't been modified are marked with
    /// `offset = BLOCK_OFFSET_PARENT` (u64::MAX) and must be read from the
    /// parent snapshot instead of the current file.
    ///
    /// # Returns
    ///
    /// `true` if `offset == u64::MAX`, indicating a parent reference.
    ///
    /// # Examples
    ///
    /// ```
    /// use hexz_core::format::index::BlockInfo;
    ///
    /// let parent_block = BlockInfo {
    ///     offset: u64::MAX,  // BLOCK_OFFSET_PARENT
    ///     length: 0,
    ///     logical_len: 4096,
    ///     checksum: 0,
    /// };
    /// assert!(parent_block.is_parent_ref());
    /// ```
    pub fn is_parent_ref(&self) -> bool {
        self.offset == u64::MAX
    }
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
/// use hexz_core::format::index::PageEntry;
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
/// use hexz_core::format::index::{MasterIndex, PageEntry};
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
/// use hexz_core::format::index::{IndexPage, BlockInfo};
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_info_sparse_creation() {
        let sparse = BlockInfo::sparse(4096);
        assert_eq!(sparse.offset, 0);
        assert_eq!(sparse.length, 0);
        assert_eq!(sparse.logical_len, 4096);
        assert_eq!(sparse.checksum, 0);
    }

    #[test]
    fn test_block_info_sparse_various_sizes() {
        for size in [128, 1024, 4096, 65536, 1048576] {
            let sparse = BlockInfo::sparse(size);
            assert_eq!(sparse.logical_len, size);
            assert!(sparse.is_sparse());
        }
    }

    #[test]
    fn test_block_info_is_sparse_true() {
        let sparse = BlockInfo::sparse(4096);
        assert!(sparse.is_sparse());

        let manual_sparse = BlockInfo {
            offset: 0,
            length: 0,
            logical_len: 4096,
            checksum: 0,
        };
        assert!(manual_sparse.is_sparse());
    }

    #[test]
    fn test_block_info_is_sparse_false_normal_block() {
        let normal = BlockInfo {
            offset: 4096,
            length: 2048,
            logical_len: 4096,
            checksum: 0x12345678,
        };
        assert!(!normal.is_sparse());
    }

    #[test]
    fn test_block_info_is_sparse_false_parent_ref() {
        let parent_ref = BlockInfo {
            offset: u64::MAX,
            length: 0,
            logical_len: 4096,
            checksum: 0,
        };
        assert!(!parent_ref.is_sparse());
    }

    #[test]
    fn test_block_info_is_parent_ref_true() {
        let parent_ref = BlockInfo {
            offset: u64::MAX,
            length: 0,
            logical_len: 4096,
            checksum: 0,
        };
        assert!(parent_ref.is_parent_ref());
    }

    #[test]
    fn test_block_info_is_parent_ref_false() {
        let normal = BlockInfo {
            offset: 4096,
            length: 2048,
            logical_len: 4096,
            checksum: 0x12345678,
        };
        assert!(!normal.is_parent_ref());

        let sparse = BlockInfo::sparse(4096);
        assert!(!sparse.is_parent_ref());
    }

    #[test]
    fn test_block_info_default() {
        let default = BlockInfo::default();
        assert_eq!(default.offset, 0);
        assert_eq!(default.length, 0);
        assert_eq!(default.logical_len, 0);
        assert_eq!(default.checksum, 0);
        assert!(default.is_sparse());
    }

    #[test]
    fn test_block_info_serialization() {
        let block = BlockInfo {
            offset: 4096,
            length: 2048,
            logical_len: 4096,
            checksum: 0x12345678,
        };

        let bytes = bincode::serialize(&block).unwrap();
        let deserialized: BlockInfo = bincode::deserialize(&bytes).unwrap();

        assert_eq!(deserialized.offset, block.offset);
        assert_eq!(deserialized.length, block.length);
        assert_eq!(deserialized.logical_len, block.logical_len);
        assert_eq!(deserialized.checksum, block.checksum);
    }

    #[test]
    fn test_page_entry_creation() {
        let entry = PageEntry {
            offset: 1048576,
            length: 65536,
            start_block: 0,
            start_logical: 0,
        };

        assert_eq!(entry.offset, 1048576);
        assert_eq!(entry.length, 65536);
        assert_eq!(entry.start_block, 0);
        assert_eq!(entry.start_logical, 0);
    }

    #[test]
    fn test_page_entry_serialization() {
        let entry = PageEntry {
            offset: 1048576,
            length: 65536,
            start_block: 100,
            start_logical: 409600,
        };

        let bytes = bincode::serialize(&entry).unwrap();
        let deserialized: PageEntry = bincode::deserialize(&bytes).unwrap();

        assert_eq!(deserialized.offset, entry.offset);
        assert_eq!(deserialized.length, entry.length);
        assert_eq!(deserialized.start_block, entry.start_block);
        assert_eq!(deserialized.start_logical, entry.start_logical);
    }

    #[test]
    fn test_master_index_default() {
        let master = MasterIndex::default();
        assert!(master.disk_pages.is_empty());
        assert!(master.memory_pages.is_empty());
        assert_eq!(master.disk_size, 0);
        assert_eq!(master.memory_size, 0);
    }

    #[test]
    fn test_master_index_with_pages() {
        let master = MasterIndex {
            disk_pages: vec![
                PageEntry {
                    offset: 4096,
                    length: 65536,
                    start_block: 0,
                    start_logical: 0,
                },
                PageEntry {
                    offset: 69632,
                    length: 65536,
                    start_block: 4096,
                    start_logical: 16777216,
                },
            ],
            memory_pages: vec![],
            disk_size: 1_000_000_000,
            memory_size: 0,
        };

        assert_eq!(master.disk_pages.len(), 2);
        assert_eq!(master.disk_size, 1_000_000_000);
    }

    #[test]
    fn test_master_index_serialization() {
        let master = MasterIndex {
            disk_pages: vec![PageEntry {
                offset: 4096,
                length: 65536,
                start_block: 0,
                start_logical: 0,
            }],
            memory_pages: vec![],
            disk_size: 1_000_000_000,
            memory_size: 0,
        };

        let bytes = bincode::serialize(&master).unwrap();
        let deserialized: MasterIndex = bincode::deserialize(&bytes).unwrap();

        assert_eq!(deserialized.disk_pages.len(), master.disk_pages.len());
        assert_eq!(deserialized.disk_size, master.disk_size);
        assert_eq!(deserialized.memory_size, master.memory_size);
    }

    #[test]
    fn test_index_page_default() {
        let page = IndexPage::default();
        assert!(page.blocks.is_empty());
    }

    #[test]
    fn test_index_page_with_blocks() {
        let page = IndexPage {
            blocks: vec![
                BlockInfo {
                    offset: 4096,
                    length: 2048,
                    logical_len: 4096,
                    checksum: 0x12345678,
                },
                BlockInfo {
                    offset: 6144,
                    length: 1024,
                    logical_len: 4096,
                    checksum: 0x9ABCDEF0,
                },
            ],
        };

        assert_eq!(page.blocks.len(), 2);
        assert_eq!(page.blocks[0].offset, 4096);
        assert_eq!(page.blocks[1].offset, 6144);
    }

    #[test]
    fn test_index_page_serialization() {
        let page = IndexPage {
            blocks: vec![
                BlockInfo {
                    offset: 4096,
                    length: 2048,
                    logical_len: 4096,
                    checksum: 0x12345678,
                },
                BlockInfo {
                    offset: 6144,
                    length: 1024,
                    logical_len: 4096,
                    checksum: 0x9ABCDEF0,
                },
            ],
        };

        let bytes = bincode::serialize(&page).unwrap();
        let deserialized: IndexPage = bincode::deserialize(&bytes).unwrap();

        assert_eq!(deserialized.blocks.len(), page.blocks.len());
        assert_eq!(deserialized.blocks[0].offset, page.blocks[0].offset);
        assert_eq!(deserialized.blocks[1].offset, page.blocks[1].offset);
    }

    #[test]
    fn test_entries_per_page_constant() {
        assert_eq!(ENTRIES_PER_PAGE, 4096);
    }
}
