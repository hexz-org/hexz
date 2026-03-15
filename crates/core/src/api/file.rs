//! High-level archive file API and logical stream types.

use crate::algo::compression::{Compressor, create_compressor};
use crate::algo::encryption::Encryptor;
use crate::cache::buffer_pool::BufferPool;
use crate::cache::lru::{BlockCache, ShardedPageCache};
use crate::cache::prefetch::Prefetcher;
use crate::format::header::Header;
use crate::format::index::{BlockInfo, IndexPage, MasterIndex, PageEntry};
use crate::format::version::{check_version, compatibility_message};
use crate::store::StorageBackend;
use bytes::Bytes;
use crc32fast::hash as crc32_hash;
use std::collections::HashMap;
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::{Arc, Mutex};

use hexz_common::constants::{BLOCK_OFFSET_PARENT, DEFAULT_BLOCK_SIZE};
use hexz_common::{Error, Result};
use rayon::prelude::*;

/// A factory function that opens a parent archive by path.
///
/// Provided by the caller of [`Archive::with_cache_and_loader`] so that the
/// core read API has no hard dependency on any specific storage backend
/// implementation. Storage crates supply a concrete loader; callers that
/// know parents cannot exist may pass `None`.
pub type ParentLoader = Box<dyn Fn(&str) -> Result<Arc<Archive>> + Send + Sync>;

/// Shared zero block for the default block size to avoid allocating when returning zero blocks.
static ZEROS_64K: [u8; DEFAULT_BLOCK_SIZE as usize] = [0u8; DEFAULT_BLOCK_SIZE as usize];

/// A map from block hash to its location in the archive.
type HashIndex = HashMap<[u8; 32], (ArchiveStream, u64, BlockInfo)>;

/// Work item for block decompression: (`block_idx`, info, `buf_offset`, `offset_in_block`, `to_copy`)
type WorkItem = (u64, BlockInfo, usize, usize, usize);

/// Result of fetching a block from cache or storage.
///
/// Eliminates TOCTOU races by tracking data state at fetch time rather than
/// re-checking the cache later (which can give a different answer if a
/// background prefetch thread modifies the cache between check and use).
enum FetchResult {
    /// Data is already decompressed (came from L1 cache or is a zero block).
    Decompressed(Bytes),
    /// Data is raw compressed bytes from storage (needs decompression).
    Compressed(Bytes),
}

/// Logical stream identifier for multi-stream archives.
///
/// Hexz archives can store independent data streams:
/// - **Main**: Primary data stream (e.g. file system image, main dataset)
/// - **Auxiliary**: Optional secondary data
///
/// # Example
///
/// ```ignore
/// use hexz_core::{Archive, ArchiveStream};
/// # use std::sync::Arc;
/// # fn example(snapshot: Arc<Archive>) -> Result<(), Box<dyn std::error::Error>> {
/// // Read 4KB from main stream
/// let data = snapshot.read_at(ArchiveStream::Main, 0, 4096)?;
///
/// // Read 4KB from auxiliary stream (if present)
/// let aux = snapshot.read_at(ArchiveStream::Auxiliary, 0, 4096)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ArchiveStream {
    /// Main data stream
    Main = 0,
    /// Auxiliary data stream
    Auxiliary = 1,
}

/// Read-only interface for accessing Hexz archive data.
///
/// `Archive` is the primary API for reading compressed, block-indexed archives.
/// It handles:
/// - **Logical-to-Physical Mapping**: Translates byte offsets to blocks via index pages.
/// - **Compression**: Transparent decompression using LZ4 or Zstandard.
/// - **Encryption**: Transparent decryption using AES-256-GCM.
/// - **Caching**: Two-level caching (L1 decompressed blocks, L2 index pages).
/// - **Thin Archives**: Resolves missing blocks from parent archives.
/// - **Prefetching**: Asynchronous background loading of sequential blocks.
///
/// # Thread Safety
///
/// `Archive` is `Send + Sync`. All methods are thread-safe and utilize sharded
/// locks to minimize contention during concurrent reads.
pub struct Archive {
    /// Archive metadata (sizes, compression, encryption settings)
    pub header: Header,

    /// Decoded metadata bytes from the metadata section
    pub metadata: Option<Vec<u8>>,

    /// Master index containing top-level page entries
    pub(crate) master: MasterIndex,

    /// Storage backend for reading raw archive data
    backend: Arc<dyn StorageBackend>,

    /// Compression algorithm (LZ4 or Zstandard)
    compressor: Box<dyn Compressor>,

    /// Optional encryption (AES-256-GCM)
    encryptor: Option<Box<dyn Encryptor>>,

    /// Optional parent archive for thin (incremental) archives.
    /// When a block's offset is `BLOCK_OFFSET_PARENT`, data is fetched from parent.
    parents: Vec<Arc<Self>>,

    /// L1 Cache: Decompressed data blocks (sharded for concurrency)
    cache_l1: Arc<BlockCache>,

    /// L2 Cache: Deserialized index pages (sharded for concurrency)
    page_cache: Arc<ShardedPageCache>,

    /// Buffer pool for reusing decompression buffers (constructed for future use)
    _buffer_pool: Arc<BufferPool>,

    /// Sequential prefetch controller
    prefetcher: Option<Arc<Prefetcher>>,

    /// Lazy hash index for resolving `ParentRef` by content rather than offset.
    hash_index: Mutex<Option<Arc<HashIndex>>>,
}

impl std::fmt::Debug for Archive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Archive")
            .field("version", &self.header.version)
            .field("block_size", &self.header.block_size)
            .field("compression", &self.header.compression)
            .field("encrypted", &self.header.encryption.is_some())
            .field("parents", &self.parents.len())
            .finish_non_exhaustive()
    }
}

impl Archive {
    /// Opens a Hexz archive with default cache settings.
    ///
    /// This is the primary constructor for `Archive`. It:
    /// 1. Reads and validates the archive header (magic bytes, version)
    /// 2. Deserializes the master index
    /// 3. Recursively loads parent archives (for thin archives)
    /// 4. Initializes block and page caches
    ///
    /// # Parameters
    ///
    /// - `backend`: Implementation of [`StorageBackend`] (Local file, S3, etc.)
    /// - `encryptor`: Optional decryptor (required if archive is encrypted)
    ///
    /// # Errors
    ///
    /// - `Error::Io`: Backend I/O failure or file not found.
    /// - `Error::Format`: Invalid magic bytes or corrupted header.
    /// - `Error::Encryption`: Missing or incorrect encryption key.
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use std::sync::Arc;
    /// # use hexz_core::Archive;
    /// # use hexz_store::local::FileBackend;
    /// let backend = Arc::new(FileBackend::new("data.hxz".as_ref())?);
    /// let archive = Archive::open(backend, None)?;
    ///
    /// println!("Main size: {} bytes", archive.size(ArchiveStream::Main));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn open(
        backend: Arc<dyn StorageBackend>,
        encryptor: Option<Box<dyn Encryptor>>,
    ) -> Result<Arc<Self>> {
        Self::open_with_cache(backend, encryptor, None, None)
    }

    /// Like [`open`](Self::open) but with custom cache capacity.
    pub fn open_with_cache(
        backend: Arc<dyn StorageBackend>,
        encryptor: Option<Box<dyn Encryptor>>,
        cache_capacity_bytes: Option<usize>,
        prefetch_window_size: Option<u32>,
    ) -> Result<Arc<Self>> {
        // 1. Read header to determine compression type and dictionary
        let header = Header::read_from_backend(backend.as_ref())?;

        // 2. Validate version
        if !check_version(header.version).is_compatible() {
            return Err(Error::Format(compatibility_message(header.version)));
        }

        // 3. Load dictionary if present
        let dictionary = header.load_dictionary(backend.as_ref())?;

        // 4. Initialize compressor
        let compressor = create_compressor(header.compression, None, dictionary.as_deref());

        // 5. Recursively open with all settings
        // Note: For now we pass None for parent loader; higher-level crates
        // like hexz-store wrap this to provide a recursive parent loader.
        Self::with_cache_and_loader(
            backend,
            compressor,
            encryptor,
            cache_capacity_bytes,
            prefetch_window_size,
            None,
        )
    }

    /// Primary constructor for manual `Archive` initialization.
    ///
    /// This is the primary constructor used by `hexz-store` to supply a
    /// configured compressor and backend.
    pub fn new(
        backend: Arc<dyn StorageBackend>,
        compressor: Box<dyn Compressor>,
        encryptor: Option<Box<dyn Encryptor>>,
    ) -> Result<Arc<Self>> {
        Self::with_cache(backend, compressor, encryptor, None, None)
    }

    /// Opens a Hexz archive with custom cache capacity and prefetching.
    pub fn with_cache(
        backend: Arc<dyn StorageBackend>,
        compressor: Box<dyn Compressor>,
        encryptor: Option<Box<dyn Encryptor>>,
        cache_capacity_bytes: Option<usize>,
        prefetch_window_size: Option<u32>,
    ) -> Result<Arc<Self>> {
        Self::with_cache_and_loader(
            backend,
            compressor,
            encryptor,
            cache_capacity_bytes,
            prefetch_window_size,
            None,
        )
    }

    /// Like [`with_cache`](Self::with_cache) but accepts an optional parent loader.
    ///
    /// The `parent_loader` is used to resolve parent archives for thin archives.
    /// If an archive declares parents but no loader is provided, blocks referring
    /// to parents will return zeros.
    pub fn with_cache_and_loader(
        backend: Arc<dyn StorageBackend>,
        compressor: Box<dyn Compressor>,
        encryptor: Option<Box<dyn Encryptor>>,
        cache_capacity_bytes: Option<usize>,
        prefetch_window_size: Option<u32>,
        parent_loader: Option<&ParentLoader>,
    ) -> Result<Arc<Self>> {
        // Read fixed header
        let header = Header::read_from_backend(backend.as_ref())?;

        // Verify encryption status match
        if header.encryption.is_some() && encryptor.is_none() {
            return Err(Error::Encryption(
                "Archive is encrypted but no encryptor was provided".into(),
            ));
        }

        // Read master index
        let master = MasterIndex::read_from_backend(backend.as_ref(), header.index_offset)?;

        // Load metadata if present
        let metadata = if let (Some(offset), Some(length)) = (header.metadata_offset, header.metadata_length) {
            Some(backend.read_exact(offset, length as usize)?.to_vec())
        } else {
            None
        };

        // Recursively load parent archives if a loader is provided.
        let mut parents = Vec::new();
        if let Some(loader) = parent_loader {
            for parent_path in &header.parent_paths {
                tracing::info!("Loading parent archive: {}", parent_path);
                parents.push(loader(parent_path)?);
            }
        } else if !header.parent_paths.is_empty() {
            tracing::warn!(
                "Archive has {} parent path(s) but no parent_loader was provided; \
                 parent-reference blocks will not be resolvable.",
                header.parent_paths.len()
            );
        }

        // Initialize caches
        let cache_l1 = Arc::new(BlockCache::with_capacity(cache_capacity_bytes.unwrap_or(
            crate::cache::lru::DEFAULT_L1_CAPACITY,
        )));
        let page_cache = Arc::new(ShardedPageCache::default());
        let buffer_pool = Arc::new(BufferPool::new(crate::cache::buffer_pool::DEFAULT_POOL_SIZE));

        // Initialize prefetcher if window size > 0
        let prefetcher = prefetch_window_size
            .filter(|&w| w > 0)
            .map(|w| Arc::new(Prefetcher::new(w)));

        Ok(Arc::new(Self {
            header,
            metadata,
            master,
            backend,
            compressor,
            encryptor,
            parents,
            cache_l1,
            page_cache,
            _buffer_pool: buffer_pool,
            prefetcher,
            hash_index: Mutex::new(None),
        }))
    }

    /// Returns the logical size of a stream in bytes.
    ///
    /// # Parameters
    ///
    /// - `stream`: The stream to query (Main or Auxiliary)
    ///
    /// # Returns
    ///
    /// The uncompressed, logical size of the stream. This is the size you would
    /// get if you decompressed all blocks and concatenated them.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use hexz_core::{Archive, ArchiveStream};
    /// # use std::sync::Arc;
    /// # fn example(archive: Arc<Archive>) {
    /// let disk_bytes = archive.size(ArchiveStream::Main);
    /// let mem_bytes = archive.size(ArchiveStream::Auxiliary);
    ///
    /// println!("Main: {} GB", disk_bytes / (1024 * 1024 * 1024));
    /// println!("Auxiliary: {} MB", mem_bytes / (1024 * 1024));
    /// # }
    /// ```
    pub const fn size(&self, stream: ArchiveStream) -> u64 {
        match stream {
            ArchiveStream::Main => self.master.main_size,
            ArchiveStream::Auxiliary => self.master.auxiliary_size,
        }
    }

    /// Returns the total number of prefetch operations spawned since this file was opened.
    /// Returns 0 if prefetching is disabled.
    pub fn prefetch_spawn_count(&self) -> u64 {
        self.prefetcher.as_ref().map_or(0, |p| p.spawn_count())
    }

    /// Reads a single block from this archive.
    pub fn read_block(
        &self,
        stream: ArchiveStream,
        block_idx: u64,
        info: &BlockInfo,
    ) -> Result<Bytes> {
        let fetch_result = self.fetch_raw_block(stream, block_idx, info)?;
        match fetch_result {
            FetchResult::Decompressed(d) => Ok(d),
            FetchResult::Compressed(raw) => self.decompress_and_verify(&raw, block_idx, info),
        }
    }

    /// Lazily builds and returns the hash index for this archive.
    fn get_hash_index(&self) -> Result<Arc<HashIndex>> {
        let mut index_guard = self.hash_index.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(index) = &*index_guard {
            return Ok(index.clone());
        }

        tracing::debug!("Building hash index for archive...");
        let mut map = HashMap::new();

        // Index main stream
        for page_entry in &self.master.main_pages {
            let page = self.get_page(page_entry)?;
            for (i, block) in page.blocks.iter().enumerate() {
                if !block.is_sparse() && block.offset != BLOCK_OFFSET_PARENT {
                    let global_idx = page_entry.start_block + i as u64;
                    _ = map.insert(block.hash, (ArchiveStream::Main, global_idx, *block));
                }
            }
        }

        // Index auxiliary stream
        for page_entry in &self.master.auxiliary_pages {
            let page = self.get_page(page_entry)?;
            for (i, block) in page.blocks.iter().enumerate() {
                if !block.is_sparse() && block.offset != BLOCK_OFFSET_PARENT {
                    let global_idx = page_entry.start_block + i as u64;
                    _ = map.insert(block.hash, (ArchiveStream::Auxiliary, global_idx, *block));
                }
            }
        }

        let index = Arc::new(map);
        *index_guard = Some(index.clone());
        drop(index_guard);
        Ok(index)
    }

    /// Finds a block in this archive by its hash.
    pub fn get_block_by_hash(
        &self,
        hash: &[u8; 32],
    ) -> Result<Option<(ArchiveStream, u64, BlockInfo)>> {
        let index = self.get_hash_index()?;
        Ok(index.get(hash).copied())
    }

    /// Iterates all non-sparse block hashes for the given stream.
    ///
    /// Used by `hexz-ops` to build a `ParentIndex` for cross-file deduplication
    /// without requiring access to private fields.
    pub fn iter_block_hashes(&self, stream: ArchiveStream) -> Result<Vec<[u8; 32]>> {
        let pages = match stream {
            ArchiveStream::Main => &self.master.main_pages,
            ArchiveStream::Auxiliary => &self.master.auxiliary_pages,
        };
        let mut hashes = Vec::new();
        for page_entry in pages {
            let page: Arc<IndexPage> = self.get_page(page_entry)?;
            for block_info in &page.blocks {
                let info: &BlockInfo = block_info;
                if !info.is_sparse() && info.hash != [0u8; 32] {
                    hashes.push(info.hash);
                }
            }
        }
        Ok(hashes)
    }

    /// Returns the block metadata for a given logical offset.
    pub fn get_block_info(
        &self,
        stream: ArchiveStream,
        offset: u64,
    ) -> Result<Option<(u64, BlockInfo)>> {
        let pages = match stream {
            ArchiveStream::Main => &self.master.main_pages,
            ArchiveStream::Auxiliary => &self.master.auxiliary_pages,
        };

        if pages.is_empty() {
            return Ok(None);
        }

        let page_idx: usize = match pages.binary_search_by(|p| p.start_logical.cmp(&offset)) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };

        let page_entry = &pages[page_idx];
        let page: Arc<IndexPage> = self.get_page(page_entry)?;
        let mut block_logical_start = page_entry.start_logical;

        for (i, block) in page.blocks.iter().enumerate() {
            let block_end = block_logical_start + block.logical_len as u64;
            if offset >= block_logical_start && offset < block_end {
                let global_idx = page_entry.start_block + i as u64;
                return Ok(Some((global_idx, *block)));
            }
            block_logical_start = block_end;
        }

        Ok(None)
    }

    /// Reads data from an archive stream at a given offset.
    ///
    /// This is the main read method for random access. It:
    /// 1. Identifies which blocks overlap the requested range
    /// 2. Fetches blocks from cache or decompresses from storage
    /// 3. Handles thin archive fallback to parent
    /// 4. Assembles the final buffer from block slices
    ///
    /// # Parameters
    ///
    /// - `stream`: Which stream to read from (Main or Auxiliary)
    /// - `offset`: Logical byte offset in the stream
    /// - `len`: Number of bytes to read
    ///
    /// # Returns
    ///
    /// A `Vec<u8>` containing the requested data. If the request extends beyond
    /// the end of the stream, it is truncated. If it starts beyond the end,
    /// an empty vector is returned.
    ///
    /// # Example
    ///
    /// ```ignore
    /// # use std::sync::Arc;
    /// # use hexz_core::{Archive, ArchiveStream};
    /// # fn example(archive: Arc<Archive>) -> hexz_common::Result<()> {
    /// // Read first 512 bytes of main stream
    /// let data = archive.read_at(ArchiveStream::Main, 0, 512)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn read_at(
        self: &Arc<Self>,
        stream: ArchiveStream,
        offset: u64,
        len: usize,
    ) -> Result<Vec<u8>> {
        let stream_size = self.size(stream);
        if offset >= stream_size {
            return Ok(Vec::new());
        }
        let actual_len = std::cmp::min(len as u64, stream_size - offset) as usize;
        let mut buffer = vec![0u8; actual_len];
        self.read_at_into(stream, offset, &mut buffer)?;
        Ok(buffer)
    }

    /// Reads into a provided buffer. Unused suffix is zero-filled. Uses parallel decompression when spanning multiple blocks.
    pub fn read_at_into(
        self: &Arc<Self>,
        stream: ArchiveStream,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<()> {
        if buffer.is_empty() {
            return Ok(());
        }
        // SAFETY: &mut [u8] and &mut [MaybeUninit<u8>] have identical layout (both
        // are slices of single-byte types). Initialized u8 values are valid MaybeUninit<u8>.
        // The borrow is derived from `buffer` so no aliasing occurs.
        let uninit = unsafe { &mut *(std::ptr::from_mut::<[u8]>(buffer) as *mut [MaybeUninit<u8>]) };
        self.read_at_into_uninit(stream, offset, uninit)
    }

    /// Minimum number of local blocks to use the parallel decompression path.
    /// Below this, serial decompression is usually faster due to thread sync overhead.
    const PARALLEL_MIN_BLOCKS: usize = 4;

    /// Collects work items for blocks that need decompression.
    /// Handles zero blocks and parent-delegated blocks by writing to target immediately.
    fn collect_work_items(
        &self,
        _stream: ArchiveStream,
        pages: &[PageEntry],
        page_idx: usize,
        target: &mut [MaybeUninit<u8>],
        offset: u64,
        actual_len: usize,
    ) -> Result<(Vec<WorkItem>, usize)> {
        let mut work_items = Vec::new();
        let mut current_pos = offset;
        let mut remaining = actual_len;
        let mut buf_offset = 0usize;

        for page_entry in pages.iter().skip(page_idx) {
            if remaining == 0 {
                break;
            }
            // Stop if the current page starts after the end of our read range
            if page_entry.start_logical > current_pos + remaining as u64 {
                break;
            }

            let page = self.get_page(page_entry)?;
            let mut block_logical_start = page_entry.start_logical;

            for (i, block) in page.blocks.iter().enumerate() {
                if remaining == 0 {
                    break;
                }
                let block_end = block_logical_start + block.logical_len as u64;

                // Check if this block overlaps with our read range
                if block_end > current_pos {
                    let offset_in_block = (current_pos - block_logical_start) as usize;
                    let to_copy = std::cmp::min(
                        remaining,
                        (block.logical_len as usize).saturating_sub(offset_in_block),
                    );

                    // CASE 1: Zero block (sparse)
                    if block.offset == 0 && block.length == 0 {
                        Self::zero_fill_uninit(&mut target[buf_offset..buf_offset + to_copy]);
                    }
                    // CASE 2: Parent block (delegation)
                    else if block.offset == BLOCK_OFFSET_PARENT {
                        let mut found = false;
                        for parent in &self.parents {
                            if let Some((p_stream, p_idx, p_info)) = parent.get_block_by_hash(&block.hash)? {
                                let data = parent.read_block(p_stream, p_idx, &p_info)?;
                                
                                // Copy the requested range from the parent block
                                let src = &data[offset_in_block..offset_in_block + to_copy];
                                // SAFETY: distinct ranges
                                unsafe {
                                    let dst_ptr = target.as_mut_ptr().add(buf_offset).cast::<u8>();
                                    ptr::copy_nonoverlapping(src.as_ptr(), dst_ptr, to_copy);
                                }
                                
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            Self::zero_fill_uninit(&mut target[buf_offset..buf_offset + to_copy]);
                        }
                    }
                    // CASE 3: Data block (local)
                    else {
                        let global_idx = page_entry.start_block + i as u64;
                        work_items.push((
                            global_idx,
                            *block,
                            buf_offset,
                            offset_in_block,
                            to_copy,
                        ));
                    }

                    current_pos += to_copy as u64;
                    remaining -= to_copy;
                    buf_offset += to_copy;
                }
                block_logical_start += block.logical_len as u64;
            }
        }

        Ok((work_items, buf_offset))
    }

    /// Executes parallel decompression for multiple blocks.
    /// Uses rayon to decompress blocks concurrently.
    fn execute_parallel_decompression(
        self: &Arc<Self>,
        stream: ArchiveStream,
        work_items: &[WorkItem],
        target: &mut [MaybeUninit<u8>],
    ) -> Result<()> {
        let target_ptr = target.as_mut_ptr() as usize;
        let results: Vec<Result<()>> = work_items
            .par_iter()
            .map(|&(block_idx, ref info, buf_offset, offset_in_block, to_copy)| {
                // Fetch and decompress
                let fetch_result = self.fetch_raw_block(stream, block_idx, info)?;
                let data = match fetch_result {
                    FetchResult::Decompressed(d) => d,
                    FetchResult::Compressed(raw) => self.decompress_and_verify(&raw, block_idx, info)?,
                };

                // Copy to target
                let src = &data[offset_in_block..offset_in_block + to_copy];
                // SAFETY: We are writing to a distinct, non-overlapping range of the target buffer
                // for each work item. buf_offset and to_copy ensure no bounds are exceeded.
                unsafe {
                    let dst_ptr = (target_ptr + buf_offset) as *mut u8;
                    ptr::copy_nonoverlapping(src.as_ptr(), dst_ptr, to_copy);
                }
                Ok(())
            })
            .collect();

        // Propagate the first error encountered, if any
        for r in results {
            r?;
        }
        Ok(())
    }

    /// Executes serial decompression for a small number of blocks.
    fn execute_serial_decompression(
        &self,
        stream: ArchiveStream,
        work_items: &[WorkItem],
        target: &mut [MaybeUninit<u8>],
    ) -> Result<()> {
        for &(block_idx, ref info, buf_offset, offset_in_block, to_copy) in work_items {
            let fetch_result = self.fetch_raw_block(stream, block_idx, info)?;
            let data = match fetch_result {
                FetchResult::Decompressed(d) => d,
                FetchResult::Compressed(raw) => self.decompress_and_verify(&raw, block_idx, info)?,
            };

            let src = &data[offset_in_block..offset_in_block + to_copy];
            // SAFETY: Serial execution, distinct ranges.
            unsafe {
                let dst_ptr = target.as_mut_ptr().add(buf_offset).cast::<u8>();
                ptr::copy_nonoverlapping(src.as_ptr(), dst_ptr, to_copy);
            }
        }
        Ok(())
    }

    /// Fills uninitialized memory with zeros.
    fn zero_fill_uninit(buffer: &mut [MaybeUninit<u8>]) {
        let mut remaining = buffer.len();
        let mut offset = 0;
        while remaining > 0 {
            let to_copy = std::cmp::min(remaining, ZEROS_64K.len());
            // SAFETY: `ZEROS_64K` is a static initialized array; `buffer` is a valid mutable
            // slice of `MaybeUninit<u8>`. `offset + to_copy <= buffer.len()` is maintained by
            // the loop, and `to_copy <= ZEROS_64K.len()`. Writing initialized bytes into
            // `MaybeUninit<u8>` is always valid since `u8` has no invalid bit patterns.
            unsafe {
                ptr::copy_nonoverlapping(
                    ZEROS_64K.as_ptr(),
                    buffer.as_mut_ptr().add(offset).cast::<u8>(),
                    to_copy,
                );
            }
            remaining -= to_copy;
            offset += to_copy;
        }
    }

    /// Writes into uninitialized memory. Unused suffix is zero-filled. Uses parallel decompression when spanning multiple blocks.
    ///
    /// **On error:** The buffer contents are undefined (possibly partially written).
    pub fn read_at_into_uninit(
        self: &Arc<Self>,
        stream: ArchiveStream,
        offset: u64,
        buffer: &mut [MaybeUninit<u8>],
    ) -> Result<()> {
        self.read_at_uninit_inner(stream, offset, buffer, false)
    }

    /// Inner implementation of [`read_at_into_uninit`](Self::read_at_into_uninit).
    /// The `is_prefetch` flag prevents recursive prefetch thread spawning:
    /// when `true`, the prefetch block is skipped to avoid unbounded thread creation.
    fn read_at_uninit_inner(
        self: &Arc<Self>,
        stream: ArchiveStream,
        offset: u64,
        buffer: &mut [MaybeUninit<u8>],
        is_prefetch: bool,
    ) -> Result<()> {
        // Early validation
        let len = buffer.len();
        if len == 0 {
            return Ok(());
        }

        let stream_size = self.size(stream);
        if offset >= stream_size {
            Self::zero_fill_uninit(buffer);
            return Ok(());
        }

        // Calculate actual read length and zero-fill suffix if needed
        let actual_len = std::cmp::min(len as u64, stream_size - offset) as usize;
        if actual_len < len {
            Self::zero_fill_uninit(&mut buffer[actual_len..]);
        }

        let target = &mut buffer[0..actual_len];

        // Get page list for stream
        let pages = match stream {
            ArchiveStream::Main => &self.master.main_pages,
            ArchiveStream::Auxiliary => &self.master.auxiliary_pages,
        };

        // Delegate to parent if no index pages
        if pages.is_empty() {
            for parent in &self.parents {
                if parent.get_block_info(stream, offset)?.is_some() {
                    return parent.read_at_into_uninit(stream, offset, target);
                }
            }
            Self::zero_fill_uninit(target);
            return Ok(());
        }

        // Find starting page index
        let page_idx: usize = match pages.binary_search_by(|p| p.start_logical.cmp(&offset)) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };

        // Collect work items (handles parent blocks, zero blocks, and queues regular blocks)
        let (work_items, buf_offset) =
            self.collect_work_items(stream, pages, page_idx, target, offset, actual_len)?;

        // Choose parallel or serial decompression based on work item count
        let work_items_slice: &[WorkItem] = &work_items;
        if work_items_slice.len() >= Self::PARALLEL_MIN_BLOCKS {
            self.execute_parallel_decompression(stream, work_items_slice, target)?;
        } else {
            self.execute_serial_decompression(stream, work_items_slice, target)?;
        }

        // Handle any remaining unprocessed data
        let remaining = actual_len - buf_offset;
        if remaining > 0 {
            let current_pos = offset + buf_offset as u64;
            let mut found = false;
            for parent in &self.parents {
                if parent.get_block_info(stream, current_pos)?.is_some() {
                    parent.read_at_into_uninit(stream, current_pos, &mut target[buf_offset..])?;
                    found = true;
                    break;
                }
            }
            if !found {
                Self::zero_fill_uninit(&mut target[buf_offset..]);
            }
        }

        // Trigger prefetch for next sequential blocks if enabled.
        // Guards:
        // 1. `is_prefetch` prevents recursive spawning (prefetch thread spawning another)
        // 2. `try_start()` limits to one in-flight prefetch at a time, preventing
        //    unbounded thread creation under rapid sequential reads
        if let Some(prefetcher) = &self.prefetcher {
            if !is_prefetch && !work_items.is_empty() && prefetcher.try_start() {
                let next_offset = offset + actual_len as u64;
                let prefetch_len = (self.header.block_size * 4) as usize;
                let snap = Arc::clone(self);
                let stream_copy = stream;
                rayon::spawn(move || {
                    let _ = snap.warm_blocks(stream_copy, next_offset, prefetch_len);
                    // Release the in-flight guard so the next read can prefetch
                    if let Some(pf) = &snap.prefetcher {
                        pf.clear_in_flight();
                    }
                });
            }
        }

        Ok(())
    }

    /// Warms the block cache for the given byte range without allocating a target buffer.
    ///
    /// Unlike [`read_at_into_uninit`](Self::read_at_into_uninit), this method only fetches,
    /// decompresses, and inserts blocks into the L1 cache. It skips blocks that are already
    /// cached, zero-length, or parent-delegated. No output buffer is allocated or written to.
    ///
    /// Used by the prefetcher to reduce overhead: the old path allocated a throwaway buffer
    /// of `block_size * 4` bytes and copied decompressed data into it, only to discard it.
    fn warm_blocks(&self, stream: ArchiveStream, offset: u64, len: usize) -> Result<()> {
        if len == 0 {
            return Ok(());
        }
        let stream_size = self.size(stream);
        if offset >= stream_size {
            return Ok(());
        }
        let actual_len = std::cmp::min(len as u64, stream_size - offset) as usize;

        let pages = match stream {
            ArchiveStream::Main => &self.master.main_pages,
            ArchiveStream::Auxiliary => &self.master.auxiliary_pages,
        };
        if pages.is_empty() {
            return Ok(());
        }

        let page_idx: usize = match pages.binary_search_by(|p| p.start_logical.cmp(&offset)) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };

        let mut current_pos = offset;
        let mut remaining = actual_len;

        for page_entry in pages.iter().skip(page_idx) {
            if remaining == 0 {
                break;
            }
            if page_entry.start_logical > current_pos + remaining as u64 {
                break;
            }

            let page: Arc<IndexPage> = self.get_page(page_entry)?;
            let mut block_logical_start = page_entry.start_logical;

            for (i, block) in page.blocks.iter().enumerate() {
                if remaining == 0 {
                    break;
                }
                let block_end = block_logical_start + block.logical_len as u64;

                if block_end > current_pos {
                    let offset_in_block = (current_pos - block_logical_start) as usize;
                    let to_advance = std::cmp::min(
                        remaining,
                        (block.logical_len as usize).saturating_sub(offset_in_block),
                    );

                    // Only warm regular blocks (skip parent-delegated and zero blocks).
                    // fetch_raw_block handles the cache check internally — on a hit it
                    // returns Decompressed which we simply ignore via the Compressed match.
                    if block.offset != BLOCK_OFFSET_PARENT && block.length > 0 {
                        let global_idx = page_entry.start_block + i as u64;
                        if let Ok(FetchResult::Compressed(raw)) =
                            self.fetch_raw_block(stream, global_idx, block)
                        {
                            if let Ok(data) = self.decompress_and_verify(&raw, global_idx, block) {
                                self.cache_l1.insert(stream, global_idx, data);
                            }
                        }
                    }

                    current_pos += to_advance as u64;
                    remaining -= to_advance;
                }
                block_logical_start += block.logical_len as u64;
            }
        }

        Ok(())
    }

    /// Like [`read_at_into_uninit`](Self::read_at_into_uninit) but accepts `&mut [u8]`. Use from FFI (e.g. Python).
    #[inline]
    pub fn read_at_into_uninit_bytes(
        self: &Arc<Self>,
        stream: ArchiveStream,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<()> {
        if buf.is_empty() {
            return Ok(());
        }
        // SAFETY: &mut [u8] and &mut [MaybeUninit<u8>] have identical layout (both
        // are slices of single-byte types). Initialized u8 values are valid MaybeUninit<u8>.
        // The borrow is derived from `buf` so no aliasing occurs.
        let uninit = unsafe { &mut *(std::ptr::from_mut::<[u8]>(buf) as *mut [MaybeUninit<u8>]) };
        self.read_at_into_uninit(stream, offset, uninit)
    }

    /// Fetches an index page from cache or storage.
    ///
    /// Index pages map logical offsets to physical block locations. This method
    /// maintains an LRU cache to avoid repeated deserialization.
    ///
    /// # Parameters
    ///
    /// - `entry`: Page metadata from master index
    ///
    /// # Returns
    ///
    /// A shared reference to the deserialized index page.
    ///
    /// # Thread Safety
    ///
    /// This method acquires a lock on the page cache only for cache lookup and insertion.
    /// I/O and deserialization are performed without holding the lock to avoid blocking
    /// other threads during cache misses.
    pub(crate) fn get_page(&self, entry: &PageEntry) -> Result<Arc<IndexPage>> {
        // Fast path: check sharded cache
        if let Some(p) = self.page_cache.get(entry.offset) {
            return Ok(p);
        }

        // Slow path: I/O and deserialization without holding any lock
        let bytes = self
            .backend
            .read_exact(entry.offset, entry.length as usize)?;
        let page: IndexPage = bincode::deserialize(&bytes)?;
        let arc = Arc::new(page);

        // Check again in case another thread inserted while we were doing I/O
        if let Some(p) = self.page_cache.get(entry.offset) {
            return Ok(p);
        }
        self.page_cache.insert(entry.offset, arc.clone());

        Ok(arc)
    }

    /// Fetches raw compressed block data from cache or storage.
    ///
    /// This is the I/O portion of block resolution, separated to enable parallel I/O.
    /// It:
    /// 1. Checks the block cache
    /// 2. Handles zero-length blocks
    /// 3. Reads raw compressed data from backend
    ///
    /// # Parameters
    ///
    /// - `stream`: Stream identifier (for cache key)
    /// - `block_idx`: Global block index
    /// - `info`: Block metadata (offset, length)
    ///
    /// # Returns
    ///
    /// Raw block data (potentially compressed/encrypted) or cached decompressed data.
    fn fetch_raw_block(
        &self,
        stream: ArchiveStream,
        block_idx: u64,
        info: &BlockInfo,
    ) -> Result<FetchResult> {
        // Check cache first - return decompressed data if available
        if let Some(data) = self.cache_l1.get(stream, block_idx) {
            return Ok(FetchResult::Decompressed(data));
        }

        // Handle zero blocks
        if info.offset == 0 && info.length == 0 {
            // Check if we can use the shared 64K zero block
            if info.logical_len == DEFAULT_BLOCK_SIZE {
                return Ok(FetchResult::Decompressed(Bytes::from_static(&ZEROS_64K)));
            }
            return Ok(FetchResult::Decompressed(Bytes::from(vec![
                0u8;
                info.logical_len as usize
            ])));
        }

        // Read raw data from backend
        let raw = self.backend.read_exact(info.offset, info.length as usize)?;
        Ok(FetchResult::Compressed(raw))
    }

    /// Decompresses and optionally decrypts a block.
    /// Validates the block checksum after decompression/decryption.
    fn decompress_and_verify(
        &self,
        raw: &[u8],
        block_idx: u64,
        info: &BlockInfo,
    ) -> Result<Bytes> {
        // Verify checksum of final data (compressed + encrypted)
        let actual_checksum = crc32_hash(raw);
        if actual_checksum != info.checksum {
            return Err(Error::Format(format!(
                "Block {} checksum mismatch: expected {:08x}, got {:08x}",
                block_idx, info.checksum, actual_checksum
            )));
        }

        let mut out = vec![0u8; info.logical_len as usize];

        if let Some(ref enc) = self.encryptor {
            let compressed = enc.decrypt(raw, block_idx)?;
            _ = self.compressor.decompress_into(&compressed, &mut out)?;
        } else {
            _ = self.compressor.decompress_into(raw, &mut out)?;
        }

        Ok(Bytes::from(out))
    }
}
