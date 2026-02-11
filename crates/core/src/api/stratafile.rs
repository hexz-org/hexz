//! High-level snapshot file API and logical stream types.

use crate::algo::compression::{Compressor, lz4::Lz4Compressor, zstd::ZstdCompressor};
use crate::algo::encryption::Encryptor;
use crate::cache::lru::BlockCache;
use crate::format::header::{CompressionType, StrataHeader};
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

use rayon::prelude::*;
use strata_common::constants::{BLOCK_OFFSET_PARENT, DEFAULT_BLOCK_SIZE, DEFAULT_ZSTD_LEVEL};
use strata_common::{Result, StrataError};

/// Shared zero block for the default block size to avoid allocating when returning zero blocks.
static ZEROS_64K: [u8; DEFAULT_BLOCK_SIZE as usize] = [0u8; DEFAULT_BLOCK_SIZE as usize];

/// Logical stream identifier for dual-stream snapshots.
///
/// Strata snapshots can store two independent data streams:
/// - **Disk**: Persistent storage (disk image, filesystem data)
/// - **Memory**: Volatile state (RAM contents, process memory)
///
/// # Example
///
/// ```no_run
/// use strata_core::{StrataFile, SnapshotStream};
/// # use std::sync::Arc;
/// # fn example(snapshot: Arc<StrataFile>) -> Result<(), Box<dyn std::error::Error>> {
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

/// Read-only interface for accessing Strata snapshot data.
///
/// `StrataFile` is the primary API for reading compressed, block-indexed snapshots.
/// It handles:
/// - Block-level decompression with LRU caching
/// - Optional AES-256-GCM decryption
/// - Thin snapshot parent chaining
/// - Dual-stream access (disk and memory)
/// - Random access with minimal I/O
///
/// # Thread Safety
///
/// `StrataFile` is `Send + Sync` and can be safely shared across threads via `Arc`.
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
/// use strata_core::{StrataFile, SnapshotStream};
/// use strata_core::store::local::FileBackend;
/// use strata_core::algo::compression::lz4::Lz4Compressor;
/// use std::sync::Arc;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let backend = Arc::new(FileBackend::new("snapshot.st".as_ref())?);
/// let compressor = Box::new(Lz4Compressor::new());
/// let snapshot = StrataFile::new(backend, compressor, None)?;
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
/// use strata_core::StrataFile;
/// use strata_core::store::local::FileBackend;
/// use strata_core::algo::compression::lz4::Lz4Compressor;
/// use std::sync::Arc;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Open base snapshot
/// let base_backend = Arc::new(FileBackend::new("base.st".as_ref())?);
/// let base = StrataFile::new(
///     base_backend,
///     Box::new(Lz4Compressor::new()),
///     None
/// )?;
///
/// // The thin snapshot will automatically load its parent based on
/// // the parent_path field in the header
/// let thin_backend = Arc::new(FileBackend::new("incremental.st".as_ref())?);
/// let thin = StrataFile::new(
///     thin_backend,
///     Box::new(Lz4Compressor::new()),
///     None
/// )?;
///
/// // Reads automatically fall back to base for unchanged blocks
/// let data = thin.read_at(strata_core::SnapshotStream::Disk, 0, 4096)?;
/// # Ok(())
/// # }
/// ```
pub struct StrataFile {
    /// Snapshot metadata (sizes, compression, encryption settings)
    pub header: StrataHeader,

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
    parent: Option<Arc<StrataFile>>,

    /// LRU cache for decompressed blocks (per-stream, per-block-index)
    cache_l1: BlockCache,

    /// LRU cache for deserialized index pages
    page_cache: Mutex<LruCache<u64, Arc<IndexPage>>>,
}

impl StrataFile {
    /// Opens a Strata snapshot with default cache settings.
    ///
    /// This is the primary constructor for `StrataFile`. It:
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
    /// - `Ok(StrataFile)` on success
    /// - `Err(StrataError::Format)` if magic bytes or version are invalid
    /// - `Err(StrataError::Io)` if storage backend fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use strata_core::{StrataFile, SnapshotStream};
    /// use strata_core::store::local::FileBackend;
    /// use strata_core::algo::compression::lz4::Lz4Compressor;
    /// use std::sync::Arc;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = Arc::new(FileBackend::new("snapshot.st".as_ref())?);
    /// let compressor = Box::new(Lz4Compressor::new());
    /// let snapshot = StrataFile::new(backend, compressor, None)?;
    ///
    /// println!("Disk size: {} bytes", snapshot.size(SnapshotStream::Disk));
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(
        backend: Arc<dyn StorageBackend>,
        compressor: Box<dyn Compressor>,
        encryptor: Option<Box<dyn Encryptor>>,
    ) -> Result<Arc<Self>> {
        Self::with_cache(backend, compressor, encryptor, None)
    }

    /// Opens a Strata snapshot with custom cache capacity.
    ///
    /// Identical to [`new`](Self::new) but allows specifying cache size in bytes.
    ///
    /// # Parameters
    ///
    /// - `backend`: Storage backend
    /// - `compressor`: Compression algorithm
    /// - `encryptor`: Optional decryption handler
    /// - `cache_capacity_bytes`: Block cache size in bytes (default: ~400MB for 4KB blocks)
    ///
    /// # Cache Sizing
    ///
    /// The cache stores decompressed blocks. Given a block size of 4KB:
    /// - `Some(100_000_000)` → ~24,000 blocks (~96MB effective)
    /// - `None` → 1000 blocks (~4MB effective)
    ///
    /// Larger caches reduce repeated decompression but increase memory usage.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use strata_core::StrataFile;
    /// use strata_core::store::local::FileBackend;
    /// use strata_core::algo::compression::lz4::Lz4Compressor;
    /// use std::sync::Arc;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let backend = Arc::new(FileBackend::new("snapshot.st".as_ref())?);
    /// let compressor = Box::new(Lz4Compressor::new());
    ///
    /// // Allocate 256MB for cache
    /// let snapshot = StrataFile::with_cache(
    ///     backend,
    ///     compressor,
    ///     None,
    ///     Some(256 * 1024 * 1024)
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_cache(
        backend: Arc<dyn StorageBackend>,
        compressor: Box<dyn Compressor>,
        encryptor: Option<Box<dyn Encryptor>>,
        cache_capacity_bytes: Option<usize>,
    ) -> Result<Arc<Self>> {
        let header_bytes = backend.read_exact(0, HEADER_SIZE)?;
        let header: StrataHeader = bincode::deserialize(&header_bytes)?;

        if &header.magic != MAGIC_BYTES {
            return Err(StrataError::Format("Invalid magic bytes".into()));
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
                return Err(StrataError::Format(compatibility_message(header.version)));
            }
        }

        let index_bytes = backend.read_exact(
            header.index_offset,
            (backend.len() - header.index_offset) as usize,
        )?;

        let master: MasterIndex = bincode::deserialize(&index_bytes)?;

        // Recursively load parent if present
        let parent = if let Some(parent_path) = &header.parent_path {
            // Note: This assumes the parent path is accessible relative to CWD
            // or is absolute. In a real system, you might need path resolution logic.
            tracing::info!("Loading parent snapshot: {}", parent_path);
            let p_backend = Arc::new(FileBackend::new(Path::new(parent_path))?);

            // For simplicity, we re-read the parent header to get its compression settings
            let p_header_bytes = p_backend.read_exact(0, HEADER_SIZE)?;
            let p_header: StrataHeader = bincode::deserialize(&p_header_bytes)?;

            let p_compressor: Box<dyn Compressor> = match p_header.compression {
                CompressionType::Lz4 => Box::new(Lz4Compressor::new()),
                CompressionType::Zstd => {
                    let dict = if let (Some(off), Some(len)) =
                        (p_header.dictionary_offset, p_header.dictionary_length)
                    {
                        Some(p_backend.read_exact(off, len as usize)?.to_vec())
                    } else {
                        None
                    };
                    Box::new(ZstdCompressor::new(DEFAULT_ZSTD_LEVEL, dict))
                }
            };

            // TODO: Handle parent encryption if needed. Assuming unencrypted parent for v1 thin snap example.
            Some(StrataFile::new(p_backend, p_compressor, None)?)
        } else {
            None
        };

        let block_size = header.block_size as usize;
        let l1_capacity = if let Some(bytes) = cache_capacity_bytes {
            (bytes / block_size).max(1)
        } else {
            1000
        };

        Ok(Arc::new(Self {
            header,
            master,
            backend,
            compressor,
            encryptor,
            parent,
            cache_l1: BlockCache::with_capacity(l1_capacity),
            page_cache: Mutex::new(LruCache::new(NonZeroUsize::new(128).unwrap())),
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
    /// use strata_core::{StrataFile, SnapshotStream};
    /// # use std::sync::Arc;
    /// # fn example(snapshot: Arc<StrataFile>) {
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
    /// - `StrataError::Io` if backend read fails (e.g. truncated file)
    /// - `StrataError::Corruption(block_idx)` if block checksum does not match
    /// - `StrataError::Decompression` if block decompression fails
    /// - `StrataError::Decryption` if block decryption fails
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
    /// use strata_core::{StrataFile, SnapshotStream};
    /// # use std::sync::Arc;
    /// # fn example(snapshot: Arc<StrataFile>) -> Result<(), Box<dyn std::error::Error>> {
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
            unsafe { ptr::write_bytes(buffer.as_mut_ptr(), 0, len) };
            return Ok(());
        }

        let actual_len = std::cmp::min(len as u64, stream_size - offset) as usize;
        if actual_len < len {
            unsafe { ptr::write_bytes(buffer[actual_len..].as_mut_ptr(), 0, len - actual_len) };
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
            unsafe { ptr::write_bytes(target.as_mut_ptr(), 0, target.len()) };
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
            let snap = Arc::clone(self);
            let target_addr = target.as_mut_ptr() as usize;
            let err: Mutex<Option<StrataError>> = Mutex::new(None);
            local_work.par_iter().for_each(
                |(block_idx, info, buf_offset, offset_in_block, to_copy)| {
                    if err.lock().unwrap().is_some() {
                        return;
                    }
                    match snap.resolve_block_data(stream, *block_idx, info) {
                        Ok(data) => {
                            let src = data.as_ref();
                            let start = *offset_in_block;
                            let len = *to_copy;
                            if start < src.len() && len <= src.len() - start {
                                let dest = (target_addr + buf_offset) as *mut u8;
                                unsafe {
                                    ptr::copy_nonoverlapping(src[start..].as_ptr(), dest, len)
                                };
                            }
                        }
                        Err(e) => {
                            let _ = (*err.lock().unwrap()).replace(e);
                        }
                    }
                },
            );
            if let Some(e) = err.lock().unwrap().take() {
                return Err(e);
            }
        } else {
            for (block_idx, info, buf_offset, offset_in_block, to_copy) in local_work {
                let data = self.resolve_block_data(stream, block_idx, &info)?;
                let src = data.as_ref();
                let start = offset_in_block;
                if start < src.len() && to_copy <= src.len() - start {
                    unsafe {
                        ptr::copy_nonoverlapping(
                            src[start..].as_ptr(),
                            target[buf_offset..].as_mut_ptr() as *mut u8,
                            to_copy,
                        );
                    }
                }
            }
        }

        if remaining > 0 {
            if let Some(parent) = &self.parent {
                parent.read_at_into_uninit(stream, current_pos, &mut target[buf_offset..])?;
            } else {
                unsafe { ptr::write_bytes(target[buf_offset..].as_mut_ptr(), 0, remaining) };
            }
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
            let mut cache = self.page_cache.lock().unwrap();
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
            let mut cache = self.page_cache.lock().unwrap();
            // Check again in case another thread inserted while we were doing I/O
            if let Some(p) = cache.get(&entry.offset) {
                return Ok(p.clone());
            }
            cache.put(entry.offset, arc.clone());
        }

        Ok(arc)
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
        if let Some(data) = self.cache_l1.get(stream, block_idx) {
            return Ok(data);
        }

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

        let raw = self.backend.read_exact(info.offset, info.length as usize)?;

        // Verify stored checksum (CRC32 of compressed/encrypted data) before decrypt/decompress
        if info.checksum != 0 {
            let computed = crc32_hash(&raw);
            if computed != info.checksum {
                return Err(StrataError::Corruption(block_idx));
            }
        }

        let decompressed = if let Some(enc) = &self.encryptor {
            let compressed = enc.decrypt(&raw, block_idx)?;
            self.compressor.decompress(&compressed)?
        } else {
            self.compressor.decompress(raw.as_ref())?
        };
        let data = Bytes::from(decompressed);

        self.cache_l1.insert(stream, block_idx, data.clone());
        Ok(data)
    }
}
