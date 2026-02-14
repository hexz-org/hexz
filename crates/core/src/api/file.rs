//! High-level snapshot file API and logical stream types.

use crate::algo::compression::{Compressor, create_compressor};
use crate::algo::encryption::Encryptor;
use crate::cache::lru::BlockCache;
use crate::cache::prefetch::Prefetcher;
use crate::format::header::Header;
use crate::format::index::{BlockInfo, IndexPage, MasterIndex, PageEntry};
use crate::format::magic::{HEADER_SIZE, MAGIC_BYTES};
use crate::format::version::{VersionCompatibility, check_version, compatibility_message};
use crate::store::StorageBackend;
use crate::store::local::file::FileBackend;
use bytes::Bytes;
use crc32fast::hash as crc32_hash;
use lru::LruCache;
use std::mem::MaybeUninit;
use std::num::NonZeroUsize;
use std::path::Path;
use std::ptr;
use std::sync::{Arc, Mutex};

use hexz_common::constants::{BLOCK_OFFSET_PARENT, DEFAULT_BLOCK_SIZE};
use hexz_common::{Error, Result};
use rayon::prelude::*;

/// Shared zero block for the default block size to avoid allocating when returning zero blocks.
static ZEROS_64K: [u8; DEFAULT_BLOCK_SIZE as usize] = [0u8; DEFAULT_BLOCK_SIZE as usize];

/// Logical stream identifier for dual-stream snapshots.
///
/// Hexz snapshots can store two independent data streams:
/// - **Disk**: Persistent storage (disk image, filesystem data)
/// - **Memory**: Volatile state (RAM contents, process memory)
///
/// # Example
///
/// ```no_run
/// use hexz_core::{File, SnapshotStream};
/// # use std::sync::Arc;
/// # fn example(snapshot: Arc<File>) -> Result<(), Box<dyn std::error::Error>> {
/// // Read 4KB from disk stream
/// let disk_data = snapshot.read_at(SnapshotStream::Disk, 0, 4096)?;
///
/// // Read 4KB from memory stream (if present)
/// let mem_data = snapshot.read_at(SnapshotStream::Memory, 0, 4096)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SnapshotStream {
    /// Persistent disk/storage stream
    Disk = 0,
    /// Volatile memory stream
    Memory = 1,
}

/// Read-only interface for accessing Hexz snapshot data.
///
/// `File` is the primary API for reading compressed, block-indexed snapshots.
/// It handles:
/// - Block-level decompression with LRU caching
/// - Optional AES-256-GCM decryption
/// - Thin snapshot parent chaining
/// - Dual-stream access (disk and memory)
/// - Random access with minimal I/O
///
/// # Thread Safety
///
/// `File` is `Send + Sync` and can be safely shared across threads via `Arc`.
/// Internal caches use `Mutex` for synchronization.
///
/// # Performance
///
/// - **Cache hit latency**: ~80μs (warm cache)
/// - **Cache miss latency**: ~1ms (cold cache, local storage)
/// - **Sequential throughput**: ~2-3 GB/s (NVMe + LZ4)
/// - **Memory overhead**: ~150MB typical (configurable)
///
/// # Examples
///
/// ## Basic Usage
///
/// ```no_run
/// use hexz_core::{File, SnapshotStream};
/// use hexz_core::store::local::FileBackend;
/// use hexz_core::algo::compression::lz4::Lz4Compressor;
/// use std::sync::Arc;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
/// let compressor = Box::new(Lz4Compressor::new());
/// let snapshot = File::new(backend, compressor, None)?;
///
/// // Read 4KB at offset 1MB
/// let data = snapshot.read_at(SnapshotStream::Disk, 1024 * 1024, 4096)?;
/// assert_eq!(data.len(), 4096);
/// # Ok(())
/// # }
/// ```
///
/// ## Thin Snapshots (with parent)
///
/// ```no_run
/// use hexz_core::File;
/// use hexz_core::store::local::FileBackend;
/// use hexz_core::algo::compression::lz4::Lz4Compressor;
/// use std::sync::Arc;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Open base snapshot
/// let base_backend = Arc::new(FileBackend::new("base.hxz".as_ref())?);
/// let base = File::new(
///     base_backend,
///     Box::new(Lz4Compressor::new()),
///     None
/// )?;
///
/// // The thin snapshot will automatically load its parent based on
/// // the parent_path field in the header
/// let thin_backend = Arc::new(FileBackend::new("incremental.hxz".as_ref())?);
/// let thin = File::new(
///     thin_backend,
///     Box::new(Lz4Compressor::new()),
///     None
/// )?;
///
/// // Reads automatically fall back to base for unchanged blocks
/// let data = thin.read_at(hexz_core::SnapshotStream::Disk, 0, 4096)?;
/// # Ok(())
/// # }
/// ```
pub struct File {
    /// Snapshot metadata (sizes, compression, encryption settings)
    pub header: Header,

    /// Master index containing top-level page entries
    master: MasterIndex,

    /// Storage backend for reading raw snapshot data
    backend: Arc<dyn StorageBackend>,

    /// Compression algorithm (LZ4 or Zstandard)
    compressor: Box<dyn Compressor>,

    /// Optional encryption (AES-256-GCM)
    encryptor: Option<Box<dyn Encryptor>>,

    /// Optional parent snapshot for thin (incremental) snapshots.
    /// When a block's offset is BLOCK_OFFSET_PARENT, data is fetched from parent.
    parent: Option<Arc<File>>,

    /// LRU cache for decompressed blocks (per-stream, per-block-index)
    cache_l1: BlockCache,

    /// LRU cache for deserialized index pages
    page_cache: Mutex<LruCache<u64, Arc<IndexPage>>>,

    /// Optional prefetcher for background data loading
    prefetcher: Option<Prefetcher>,
}

impl File {
    /// Opens a Hexz snapshot with default cache settings.
    ///
    /// This is the primary constructor for `File`. It:
    /// 1. Reads and validates the snapshot header (magic bytes, version)
    /// 2. Deserializes the master index
    /// 3. Recursively loads parent snapshots (for thin snapshots)
    /// 4. Initializes block and page caches
    ///
    /// # Parameters
    ///
    /// - `backend`: Storage backend (local file, HTTP, S3, etc.)
    /// - `compressor`: Compression algorithm matching the snapshot format
    /// - `encryptor`: Optional decryption handler (pass `None` for unencrypted snapshots)
    ///
    /// # Returns
    ///
    /// - `Ok(File)` on success
    /// - `Err(Error::Format)` if magic bytes or version are invalid
    /// - `Err(Error::Io)` if storage backend fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use hexz_core::{File, SnapshotStream};
    /// use hexz_core::store::local::FileBackend;
    /// use hexz_core::algo::compression::lz4::Lz4Compressor;
    /// use std::sync::Arc;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
    /// let compressor = Box::new(Lz4Compressor::new());
    /// let snapshot = File::new(backend, compressor, None)?;
    ///
    /// println!("Disk size: {} bytes", snapshot.size(SnapshotStream::Disk));
    /// # Ok(())
    /// # }
    /// ```
    /// Opens a snapshot, auto-detecting compression and dictionary from the header.
    ///
    /// This eliminates the 3-step boilerplate of: read header, load dict, create
    /// compressor. Equivalent to `File::new(backend, auto_compressor, encryptor)`.
    pub fn open(
        backend: Arc<dyn StorageBackend>,
        encryptor: Option<Box<dyn Encryptor>>,
    ) -> Result<Arc<Self>> {
        Self::open_with_cache(backend, encryptor, None, None)
    }

    /// Like [`open`](Self::open) but with custom cache and prefetch settings.
    pub fn open_with_cache(
        backend: Arc<dyn StorageBackend>,
        encryptor: Option<Box<dyn Encryptor>>,
        cache_capacity_bytes: Option<usize>,
        prefetch_window_size: Option<u32>,
    ) -> Result<Arc<Self>> {
        let header = Header::read_from_backend(backend.as_ref())?;
        let dictionary = header.load_dictionary(backend.as_ref())?;
        let compressor = create_compressor(header.compression, None, dictionary);
        Self::with_cache(
            backend,
            compressor,
            encryptor,
            cache_capacity_bytes,
            prefetch_window_size,
        )
    }

    pub fn new(
        backend: Arc<dyn StorageBackend>,
        compressor: Box<dyn Compressor>,
        encryptor: Option<Box<dyn Encryptor>>,
    ) -> Result<Arc<Self>> {
        Self::with_cache(backend, compressor, encryptor, None, None)
    }

    /// Opens a Hexz snapshot with custom cache capacity and prefetching.
    ///
    /// Identical to [`new`](Self::new) but allows specifying cache size and prefetch window.
    ///
    /// # Parameters
    ///
    /// - `backend`: Storage backend
    /// - `compressor`: Compression algorithm
    /// - `encryptor`: Optional decryption handler
    /// - `cache_capacity_bytes`: Block cache size in bytes (default: ~400MB for 4KB blocks)
    /// - `prefetch_window_size`: Number of blocks to prefetch ahead (default: disabled)
    ///
    /// # Cache Sizing
    ///
    /// The cache stores decompressed blocks. Given a block size of 4KB:
    /// - `Some(100_000_000)` → ~24,000 blocks (~96MB effective)
    /// - `None` → 1000 blocks (~4MB effective)
    ///
    /// Larger caches reduce repeated decompression but increase memory usage.
    ///
    /// # Prefetching
    ///
    /// When `prefetch_window_size` is set, the system will automatically fetch the next N blocks
    /// in the background after each read, optimizing sequential access patterns:
    /// - `Some(4)` → Prefetch 4 blocks ahead
    /// - `None` or `Some(0)` → Disable prefetching
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use hexz_core::File;
    /// use hexz_core::store::local::FileBackend;
    /// use hexz_core::algo::compression::lz4::Lz4Compressor;
    /// use std::sync::Arc;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = Arc::new(FileBackend::new("snapshot.hxz".as_ref())?);
    /// let compressor = Box::new(Lz4Compressor::new());
    ///
    /// // Allocate 256MB for cache, prefetch 4 blocks ahead
    /// let snapshot = File::with_cache(
    ///     backend,
    ///     compressor,
    ///     None,
    ///     Some(256 * 1024 * 1024),
    ///     Some(4)
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_cache(
        backend: Arc<dyn StorageBackend>,
        compressor: Box<dyn Compressor>,
        encryptor: Option<Box<dyn Encryptor>>,
        cache_capacity_bytes: Option<usize>,
        prefetch_window_size: Option<u32>,
    ) -> Result<Arc<Self>> {
        let header_bytes = backend.read_exact(0, HEADER_SIZE)?;
        let header: Header = bincode::deserialize(&header_bytes)?;

        if &header.magic != MAGIC_BYTES {
            return Err(Error::Format("Invalid magic bytes".into()));
        }

        // Check version compatibility
        let compatibility = check_version(header.version);
        match compatibility {
            VersionCompatibility::Full => {
                // Perfect match, proceed silently
            }
            VersionCompatibility::Degraded => {
                // Newer version, issue warning but allow
                tracing::warn!("{}", compatibility_message(header.version));
            }
            VersionCompatibility::Incompatible => {
                // Too old or too new, reject
                return Err(Error::Format(compatibility_message(header.version)));
            }
        }

        let index_bytes = backend.read_exact(
            header.index_offset,
            (backend.len() - header.index_offset) as usize,
        )?;

        let master: MasterIndex = bincode::deserialize(&index_bytes)?;

        // Recursively load parent if present
        let parent = if let Some(parent_path) = &header.parent_path {
            tracing::info!("Loading parent snapshot: {}", parent_path);
            let p_backend = Arc::new(FileBackend::new(Path::new(parent_path))?);
            Some(File::open(p_backend, None)?)
        } else {
            None
        };

        let block_size = header.block_size as usize;
        let l1_capacity = if let Some(bytes) = cache_capacity_bytes {
            (bytes / block_size).max(1)
        } else {
            1000
        };

        // Initialize prefetcher if window size is specified and > 0
        let prefetcher = prefetch_window_size.filter(|&w| w > 0).map(Prefetcher::new);

        Ok(Arc::new(Self {
            header,
            master,
            backend,
            compressor,
            encryptor,
            parent,
            cache_l1: BlockCache::with_capacity(l1_capacity),
            page_cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(128).unwrap_or(NonZeroUsize::MIN),
            )),
            prefetcher,
        }))
    }

    /// Returns the logical size of a stream in bytes.
    ///
    /// # Parameters
    ///
    /// - `stream`: The stream to query (Disk or Memory)
    ///
    /// # Returns
    ///
    /// The uncompressed, logical size of the stream. This is the size you would
    /// get if you decompressed all blocks and concatenated them.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use hexz_core::{File, SnapshotStream};
    /// # use std::sync::Arc;
    /// # fn example(snapshot: Arc<File>) {
    /// let disk_bytes = snapshot.size(SnapshotStream::Disk);
    /// let mem_bytes = snapshot.size(SnapshotStream::Memory);
    ///
    /// println!("Disk: {} GB", disk_bytes / (1024 * 1024 * 1024));
    /// println!("Memory: {} MB", mem_bytes / (1024 * 1024));
    /// # }
    /// ```
    pub fn size(&self, stream: SnapshotStream) -> u64 {
        match stream {
            SnapshotStream::Disk => self.master.disk_size,
            SnapshotStream::Memory => self.master.memory_size,
        }
    }

    /// Reads data from a snapshot stream at a given offset.
    ///
    /// This is the primary read method for random access. It:
    /// 1. Identifies which blocks overlap the requested range
    /// 2. Fetches blocks from cache or decompresses from storage
    /// 3. Handles thin snapshot fallback to parent
    /// 4. Assembles the final buffer from block slices
    ///
    /// # Parameters
    ///
    /// - `stream`: Which stream to read from (Disk or Memory)
    /// - `offset`: Starting byte offset (0-indexed)
    /// - `len`: Number of bytes to read
    ///
    /// # Returns
    ///
    /// A `Vec<u8>` containing up to `len` bytes. The returned vector may be shorter
    /// if:
    /// - `offset` is beyond the stream size (returns empty vector)
    /// - `offset + len` exceeds stream size (returns partial data)
    ///
    /// Missing data (sparse regions) is zero-filled.
    ///
    /// # Errors
    ///
    /// - `Error::Io` if backend read fails (e.g. truncated file)
    /// - `Error::Corruption(block_idx)` if block checksum does not match
    /// - `Error::Decompression` if block decompression fails
    /// - `Error::Decryption` if block decryption fails
    ///
    /// # Performance
    ///
    /// - **Cache hit**: ~80μs latency, no I/O
    /// - **Cache miss**: ~1ms latency (local storage), includes decompression
    /// - **Remote storage**: Latency depends on network (HTTP: ~50ms, S3: ~100ms)
    ///
    /// Aligned reads (offset % block_size == 0) are most efficient.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use hexz_core::{File, SnapshotStream};
    /// # use std::sync::Arc;
    /// # fn example(snapshot: Arc<File>) -> Result<(), Box<dyn std::error::Error>> {
    /// // Read first 512 bytes of disk stream
    /// let boot_sector = snapshot.read_at(SnapshotStream::Disk, 0, 512)?;
    ///
    /// // Read from arbitrary offset
    /// let chunk = snapshot.read_at(SnapshotStream::Disk, 1024 * 1024, 4096)?;
    ///
    /// // Reading beyond stream size returns empty vector
    /// let empty = snapshot.read_at(SnapshotStream::Disk, u64::MAX, 100)?;
    /// assert!(empty.is_empty());
    /// # Ok(())
    /// # }
    /// ```
    /// Reads a byte range. Uses parallel block decompression when the range spans multiple blocks.
    pub fn read_at(
        self: &Arc<Self>,
        stream: SnapshotStream,
        offset: u64,
        len: usize,
    ) -> Result<Vec<u8>> {
        let stream_size = self.size(stream);
        if offset >= stream_size {
            return Ok(Vec::new());
        }
        let actual_len = std::cmp::min(len as u64, stream_size - offset) as usize;
        if actual_len == 0 {
            return Ok(Vec::new());
        }

        let pages = match stream {
            SnapshotStream::Disk => &self.master.disk_pages,
            SnapshotStream::Memory => &self.master.memory_pages,
        };

        if pages.is_empty() {
            if let Some(parent) = &self.parent {
                return parent.read_at(stream, offset, actual_len);
            }
            return Ok(vec![0u8; actual_len]);
        }

        let mut buf: Vec<MaybeUninit<u8>> = Vec::new();
        buf.resize_with(actual_len, MaybeUninit::uninit);
        self.read_at_into_uninit(stream, offset, &mut buf)?;
        let ptr = buf.as_mut_ptr().cast::<u8>();
        let len = buf.len();
        let cap = buf.capacity();
        std::mem::forget(buf);
        // SAFETY: `buf` was a Vec<MaybeUninit<u8>> that we fully initialized via
        // `read_at_into_uninit` (which writes every byte). We `forget` the original
        // Vec to avoid a double-free and reconstruct it with the same ptr/len/cap.
        // MaybeUninit<u8> has the same layout as u8.
        Ok(unsafe { Vec::from_raw_parts(ptr, len, cap) })
    }

    /// Reads into a provided buffer. Unused suffix is zero-filled. Uses parallel decompression when spanning multiple blocks.
    pub fn read_at_into(
        self: &Arc<Self>,
        stream: SnapshotStream,
        offset: u64,
        buffer: &mut [u8],
    ) -> Result<()> {
        let len = buffer.len();
        if len == 0 {
            return Ok(());
        }
        let stream_size = self.size(stream);
        if offset >= stream_size {
            buffer.fill(0);
            return Ok(());
        }
        let actual_len = std::cmp::min(len as u64, stream_size - offset) as usize;
        if actual_len < len {
            buffer[actual_len..].fill(0);
        }
        self.read_at_into_uninit_bytes(stream, offset, &mut buffer[0..actual_len])
    }

    /// Minimum number of local blocks to use the parallel decompression path.
    const PARALLEL_MIN_BLOCKS: usize = 2;

    /// Zero-fills a slice of uninitialized memory.
    ///
    /// This helper centralizes all unsafe zero-filling operations to improve
    /// safety auditing and reduce code duplication.
    ///
    /// # Safety
    ///
    /// This function writes zeros to the provided buffer, making it fully initialized.
    /// The caller must ensure the buffer is valid for writes.
    #[inline]
    fn zero_fill_uninit(buffer: &mut [MaybeUninit<u8>]) {
        if !buffer.is_empty() {
            // SAFETY: buffer is a valid &mut [MaybeUninit<u8>] slice, so writing
            // buffer.len() zero bytes through its pointer is in-bounds.
            unsafe { ptr::write_bytes(buffer.as_mut_ptr(), 0, buffer.len()) };
        }
    }

    /// Writes into uninitialized memory. Unused suffix is zero-filled. Uses parallel decompression when spanning multiple blocks.
    ///
    /// **On error:** The buffer contents are undefined (possibly partially written).
    pub fn read_at_into_uninit(
        self: &Arc<Self>,
        stream: SnapshotStream,
        offset: u64,
        buffer: &mut [MaybeUninit<u8>],
    ) -> Result<()> {
        let len = buffer.len();
        if len == 0 {
            return Ok(());
        }

        let stream_size = self.size(stream);
        if offset >= stream_size {
            Self::zero_fill_uninit(buffer);
            return Ok(());
        }

        let actual_len = std::cmp::min(len as u64, stream_size - offset) as usize;
        if actual_len < len {
            Self::zero_fill_uninit(&mut buffer[actual_len..]);
        }

        let target = &mut buffer[0..actual_len];
        let mut buf_offset = 0;
        let mut current_pos = offset;
        let mut remaining = actual_len;

        let pages = match stream {
            SnapshotStream::Disk => &self.master.disk_pages,
            SnapshotStream::Memory => &self.master.memory_pages,
        };

        if pages.is_empty() {
            if let Some(parent) = &self.parent {
                return parent.read_at_into_uninit(stream, offset, target);
            }
            Self::zero_fill_uninit(target);
            return Ok(());
        }

        let page_idx = match pages.binary_search_by(|p| p.start_logical.cmp(&offset)) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };

        // (block_idx, info, buf_offset, offset_in_block, to_copy)
        let mut local_work: Vec<(u64, BlockInfo, usize, usize, usize)> = Vec::new();

        for page_entry in pages.iter().skip(page_idx) {
            if remaining == 0 {
                break;
            }
            if page_entry.start_logical > current_pos + remaining as u64 {
                break;
            }

            let page = self.get_page(page_entry)?;
            let mut block_logical_start = page_entry.start_logical;

            for (block_idx_in_page, block) in page.blocks.iter().enumerate() {
                let block_end = block_logical_start + block.logical_len as u64;

                if block_end > current_pos {
                    let global_block_idx = page_entry.start_block + block_idx_in_page as u64;

                    if block.offset == BLOCK_OFFSET_PARENT {
                        if let Some(parent) = &self.parent {
                            let offset_in_block = (current_pos - block_logical_start) as usize;
                            let to_copy = std::cmp::min(
                                remaining,
                                (block.logical_len as usize).saturating_sub(offset_in_block),
                            );
                            let dest = &mut target[buf_offset..buf_offset + to_copy];
                            parent.read_at_into_uninit(stream, current_pos, dest)?;
                            current_pos += to_copy as u64;
                            buf_offset += to_copy;
                            remaining -= to_copy;
                        } else {
                            let offset_in_block = (current_pos - block_logical_start) as usize;
                            let to_copy = std::cmp::min(
                                remaining,
                                (block.logical_len as usize).saturating_sub(offset_in_block),
                            );
                            // SAFETY: The sub-slice has exactly `to_copy` elements,
                            // so zeroing that many bytes is in-bounds.
                            unsafe {
                                ptr::write_bytes(
                                    target[buf_offset..buf_offset + to_copy].as_mut_ptr(),
                                    0,
                                    to_copy,
                                )
                            };
                            current_pos += to_copy as u64;
                            buf_offset += to_copy;
                            remaining -= to_copy;
                        }
                    } else if block.length == 0 {
                        let offset_in_block = (current_pos - block_logical_start) as usize;
                        let to_copy = std::cmp::min(
                            remaining,
                            (block.logical_len as usize).saturating_sub(offset_in_block),
                        );
                        // SAFETY: The sub-slice has exactly `to_copy` elements,
                        // so zeroing that many bytes is in-bounds.
                        unsafe {
                            ptr::write_bytes(
                                target[buf_offset..buf_offset + to_copy].as_mut_ptr(),
                                0,
                                to_copy,
                            )
                        };
                        current_pos += to_copy as u64;
                        buf_offset += to_copy;
                        remaining -= to_copy;
                    } else {
                        let offset_in_block = (current_pos - block_logical_start) as usize;
                        let to_copy = std::cmp::min(
                            remaining,
                            (block.logical_len as usize).saturating_sub(offset_in_block),
                        );
                        if to_copy > 0 {
                            local_work.push((
                                global_block_idx,
                                *block,
                                buf_offset,
                                offset_in_block,
                                to_copy,
                            ));
                            buf_offset += to_copy;
                            current_pos += to_copy as u64;
                            remaining -= to_copy;
                        }
                    }

                    if remaining == 0 {
                        break;
                    }
                }
                block_logical_start += block.logical_len as u64;
            }
        }

        if local_work.len() >= Self::PARALLEL_MIN_BLOCKS {
            // Two-phase parallel I/O optimization:
            // Phase 1: Parallel I/O - fetch all raw blocks concurrently
            // Phase 2: Parallel CPU - decompress and copy to target buffer
            let snap = Arc::clone(self);
            let target_addr = target.as_mut_ptr() as usize;

            // Phase 1: Parallel fetch all raw blocks
            let raw_blocks: Vec<Result<Bytes>> = local_work
                .par_iter()
                .map(|(block_idx, info, _, _, _)| snap.fetch_raw_block(stream, *block_idx, info))
                .collect();

            // Phase 2: Parallel decompress and copy
            let err: Mutex<Option<Error>> = Mutex::new(None);
            local_work
                .par_iter()
                .zip(raw_blocks)
                .for_each(|(work_item, raw_result)| {
                    if err.lock().map_or(true, |e| e.is_some()) {
                        return;
                    }

                    let (block_idx, info, buf_offset, offset_in_block, to_copy) = work_item;

                    // Handle fetch errors
                    let raw = match raw_result {
                        Ok(r) => r,
                        Err(e) => {
                            if let Ok(mut guard) = err.lock() {
                                let _ = guard.replace(e);
                            }
                            return;
                        }
                    };

                    // If cache hit or zero block, raw is already decompressed
                    let data =
                        if info.length == 0 || snap.cache_l1.get(stream, *block_idx).is_some() {
                            raw
                        } else {
                            // Decompress and verify
                            match snap.decompress_and_verify(raw, *block_idx, info) {
                                Ok(d) => {
                                    // Cache the result
                                    snap.cache_l1.insert(stream, *block_idx, d.clone());
                                    d
                                }
                                Err(e) => {
                                    if let Ok(mut guard) = err.lock() {
                                        let _ = guard.replace(e);
                                    }
                                    return;
                                }
                            }
                        };

                    // Copy to target buffer
                    let src = data.as_ref();
                    let start = *offset_in_block;
                    let len = *to_copy;
                    if start < src.len() && len <= src.len() - start {
                        // Defensive assertion: ensure destination write is within bounds
                        debug_assert!(
                            buf_offset + len <= actual_len,
                            "Buffer overflow: attempting to write {} bytes at offset {} into buffer of length {}",
                            len,
                            buf_offset,
                            actual_len
                        );
                        let dest = (target_addr + buf_offset) as *mut u8;
                        // SAFETY: `src[start..start+len]` is in-bounds (checked above).
                        // `dest` points into the `target` MaybeUninit buffer at a unique
                        // non-overlapping offset (each work item has a distinct `buf_offset`),
                        // and the rayon par_iter ensures each item writes to a disjoint region.
                        // The debug_assert above validates buf_offset + len <= actual_len.
                        unsafe { ptr::copy_nonoverlapping(src[start..].as_ptr(), dest, len) };
                    }
                });

            if let Some(e) = err.lock().ok().and_then(|mut guard| guard.take()) {
                return Err(e);
            }
        } else {
            for (block_idx, info, buf_offset, offset_in_block, to_copy) in &local_work {
                let data = self.resolve_block_data(stream, *block_idx, info)?;
                let src = data.as_ref();
                let start = *offset_in_block;
                if start < src.len() && *to_copy <= src.len() - start {
                    // Defensive assertion: ensure destination write is within bounds
                    debug_assert!(
                        *buf_offset + *to_copy <= actual_len,
                        "Buffer overflow: attempting to write {} bytes at offset {} into buffer of length {}",
                        to_copy,
                        buf_offset,
                        actual_len
                    );
                    // SAFETY: `src[start..start+to_copy]` is in-bounds (checked above).
                    // `target[buf_offset..]` has sufficient room because `buf_offset + to_copy`
                    // never exceeds `actual_len` (tracked during work-item collection).
                    // The debug_assert above validates this invariant.
                    // MaybeUninit<u8> has the same layout as u8.
                    unsafe {
                        ptr::copy_nonoverlapping(
                            src[start..].as_ptr(),
                            target[*buf_offset..].as_mut_ptr() as *mut u8,
                            *to_copy,
                        );
                    }
                }
            }
        }

        if remaining > 0 {
            if let Some(parent) = &self.parent {
                parent.read_at_into_uninit(stream, current_pos, &mut target[buf_offset..])?;
            } else {
                Self::zero_fill_uninit(&mut target[buf_offset..]);
            }
        }

        // Trigger prefetch for next sequential blocks if enabled
        if self.prefetcher.is_some() && !local_work.is_empty() {
            // Calculate the next logical position (end of current read)
            let next_offset = offset + actual_len as u64;
            let prefetch_len = (self.header.block_size * 4) as usize; // Prefetch ~4 blocks worth

            // Spawn background prefetch for the next range
            let snap = Arc::clone(self);
            let stream_copy = stream;
            std::thread::spawn(move || {
                // Prefetch silently - ignore errors
                let _ = snap.read_at(stream_copy, next_offset, prefetch_len);
            });
        }

        Ok(())
    }

    /// Like [`read_at_into_uninit`](Self::read_at_into_uninit) but accepts `&mut [u8]`. Use from FFI (e.g. Python).
    #[inline]
    pub fn read_at_into_uninit_bytes(
        self: &Arc<Self>,
        stream: SnapshotStream,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<()> {
        if buf.is_empty() {
            return Ok(());
        }
        // SAFETY: &mut [u8] and &mut [MaybeUninit<u8>] have identical layout (both
        // are slices of single-byte types). Initialized u8 values are valid MaybeUninit<u8>.
        // The borrow is derived from `buf` so no aliasing occurs.
        let uninit = unsafe { &mut *(buf as *mut [u8] as *mut [MaybeUninit<u8>]) };
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
    fn get_page(&self, entry: &PageEntry) -> Result<Arc<IndexPage>> {
        // Fast path: check cache with lock held
        {
            let mut cache = self
                .page_cache
                .lock()
                .map_err(|_| Error::Io(std::io::Error::other("Page cache lock poisoned")))?;
            if let Some(p) = cache.get(&entry.offset) {
                return Ok(p.clone());
            }
        }

        // Slow path: release lock before I/O and deserialization
        let bytes = self
            .backend
            .read_exact(entry.offset, entry.length as usize)?;
        let page: IndexPage = bincode::deserialize(&bytes)?;
        let arc = Arc::new(page);

        // Re-acquire lock only for insertion
        {
            let mut cache = self
                .page_cache
                .lock()
                .map_err(|_| Error::Io(std::io::Error::other("Page cache lock poisoned")))?;
            // Check again in case another thread inserted while we were doing I/O
            if let Some(p) = cache.get(&entry.offset) {
                return Ok(p.clone());
            }
            cache.put(entry.offset, arc.clone());
        }

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
        stream: SnapshotStream,
        block_idx: u64,
        info: &BlockInfo,
    ) -> Result<Bytes> {
        // Check cache first - return decompressed data if available
        if let Some(data) = self.cache_l1.get(stream, block_idx) {
            return Ok(data);
        }

        // Handle zero blocks
        if info.length == 0 {
            let len = info.logical_len as usize;
            if len == 0 {
                return Ok(Bytes::new());
            }
            if len == ZEROS_64K.len() {
                return Ok(Bytes::from_static(&ZEROS_64K));
            }
            return Ok(Bytes::from(vec![0u8; len]));
        }

        // Fetch raw compressed data (THIS IS THE PARALLEL PART)
        self.backend.read_exact(info.offset, info.length as usize)
    }

    /// Decompresses and verifies a raw block.
    ///
    /// This is the CPU portion of block resolution, separated to enable parallel decompression.
    /// It:
    /// 1. Verifies CRC32 checksum
    /// 2. Decrypts (if encrypted)
    /// 3. Decompresses
    ///
    /// # Parameters
    ///
    /// - `raw`: Raw block data (potentially compressed/encrypted)
    /// - `block_idx`: Global block index (for error reporting and decryption)
    /// - `info`: Block metadata (checksum)
    ///
    /// # Returns
    ///
    /// Decompressed block data as `Bytes`.
    ///
    /// # Performance
    ///
    /// Decompression throughput:
    /// - LZ4: ~2 GB/s per core
    /// - Zstd: ~500 MB/s per core
    fn decompress_and_verify(&self, raw: Bytes, block_idx: u64, info: &BlockInfo) -> Result<Bytes> {
        // Verify stored checksum (CRC32 of compressed/encrypted data) before decrypt/decompress
        if info.checksum != 0 {
            let computed = crc32_hash(&raw);
            if computed != info.checksum {
                return Err(Error::Corruption(block_idx));
            }
        }

        // Decrypt and decompress
        let decompressed = if let Some(enc) = &self.encryptor {
            let compressed = enc.decrypt(&raw, block_idx)?;
            self.compressor.decompress(&compressed)?
        } else {
            self.compressor.decompress(raw.as_ref())?
        };

        Ok(Bytes::from(decompressed))
    }

    /// Resolves raw block data by fetching from cache or decompressing from storage.
    ///
    /// This is the core decompression path. It:
    /// 1. Checks the block cache
    /// 2. Reads compressed block from backend
    /// 3. Verifies CRC32 checksum (if stored) and returns `Corruption(block_idx)` on mismatch
    /// 4. Decrypts (if encrypted)
    /// 5. Decompresses
    /// 6. Caches the result
    ///
    /// # Parameters
    ///
    /// - `stream`: Stream identifier (for cache key)
    /// - `block_idx`: Global block index
    /// - `info`: Block metadata (offset, length, compression)
    ///
    /// # Returns
    ///
    /// Decompressed block data as `Bytes` (zero-copy on cache hit).
    ///
    /// # Performance
    ///
    /// This method is hot path for cache misses. Decompression throughput:
    /// - LZ4: ~2 GB/s per core
    /// - Zstd: ~500 MB/s per core
    fn resolve_block_data(
        &self,
        stream: SnapshotStream,
        block_idx: u64,
        info: &BlockInfo,
    ) -> Result<Bytes> {
        // Fetch raw block (from cache or I/O)
        let raw = self.fetch_raw_block(stream, block_idx, info)?;

        // If cache hit or zero block, raw is already decompressed
        if info.length == 0 || self.cache_l1.get(stream, block_idx).is_some() {
            return Ok(raw);
        }

        // Decompress and verify
        let data = self.decompress_and_verify(raw, block_idx, info)?;

        // Cache the result
        self.cache_l1.insert(stream, block_idx, data.clone());
        Ok(data)
    }
}
