//! Low-level write operations for Strata snapshots.
//!
//! This module provides the foundational building blocks for writing compressed,
//! encrypted, and deduplicated blocks to snapshot files. These functions implement
//! the core write semantics used by higher-level pack operations while remaining
//! independent of the packing workflow.
//!
//! # Module Purpose
//!
//! The write operations module serves as the bridge between the high-level packing
//! pipeline and the raw file I/O layer. It encapsulates the logic for:
//!
//! - **Block Writing**: Transform raw chunks into compressed, encrypted blocks
//! - **Deduplication**: Detect and eliminate redundant blocks via content hashing
//! - **Zero Optimization**: Handle sparse data efficiently without storage
//! - **Metadata Generation**: Create BlockInfo descriptors for index building
//!
//! # Design Philosophy
//!
//! These functions are designed to be composable, stateless, and easily testable.
//! They operate on raw byte buffers and writers without knowledge of the broader
//! packing context (progress reporting, stream management, index organization).
//!
//! This separation enables:
//! - Unit testing of write logic in isolation
//! - Reuse in different packing strategies (single-stream, multi-threaded, streaming)
//! - Clear separation of concerns (write vs. orchestration)
//!
//! # Write Operation Semantics
//!
//! ## Block Transformation Pipeline
//!
//! Each block undergoes a multi-stage transformation before being written:
//!
//! ```text
//! Raw Chunk (input)
//!      ↓
//! ┌────────────────┐
//! │ Compression    │ → Compress using LZ4 or Zstd
//! └────────────────┘   (reduces size, increases CPU)
//!      ↓
//! ┌────────────────┐
//! │ Encryption     │ → Optional AES-256-GCM with block_idx nonce
//! └────────────────┘   (confidentiality + integrity)
//!      ↓
//! ┌────────────────┐
//! │ Checksum       │ → CRC32 of final data (fast integrity check)
//! └────────────────┘
//!      ↓
//! ┌────────────────┐
//! │ Deduplication  │ → SHA-256 hash lookup (skip write if duplicate)
//! └────────────────┘   (disabled for encrypted data)
//!      ↓
//! ┌────────────────┐
//! │ Write          │ → Append to output file at current offset
//! └────────────────┘
//!      ↓
//! BlockInfo (metadata: offset, length, checksum)
//! ```
//!
//! ## Write Behavior and Atomicity
//!
//! ### Single Block Writes
//!
//! Individual block writes via [`write_block`] are atomic with respect to the
//! underlying file system's write atomicity guarantees:
//!
//! - **Buffered writes**: Data passes through OS page cache
//! - **No fsync**: Writes are not flushed to disk until the writer is closed
//! - **Partial write handling**: Writer's `write_all` ensures complete writes or error
//! - **Crash behavior**: Partial blocks may be written if process crashes mid-write
//!
//! ### Deduplication State
//!
//! The deduplication map is maintained externally (by the caller). This design allows:
//! - **Flexibility**: Caller controls when/if to enable deduplication
//! - **Memory control**: Map lifetime and size managed by orchestration layer
//! - **Consistency**: Map updates are immediately visible to subsequent writes
//!
//! ### Offset Management
//!
//! The `current_offset` parameter is updated atomically after each successful write.
//! This ensures:
//! - **Sequential allocation**: Blocks are laid out contiguously in file
//! - **No gaps**: Every byte between header and master index is utilized
//! - **Predictable layout**: Physical offset increases monotonically
//!
//! ## Block Allocation Strategy
//!
//! Blocks are allocated sequentially in the order they are written:
//!
//! ```text
//! File Layout:
//! ┌──────────────┬──────────┬──────────┬──────────┬─────────────┐
//! │ Header (512B)│ Block 0  │ Block 1  │ Block 2  │ Index Pages │
//! └──────────────┴──────────┴──────────┴──────────┴─────────────┘
//!  ↑             ↑          ↑          ↑
//!  0             512        512+len0   512+len0+len1
//!
//! current_offset advances after each write:
//! - Initial: 512 (after header)
//! - After Block 0: 512 + len0
//! - After Block 1: 512 + len0 + len1
//! - After Block 2: 512 + len0 + len1 + len2
//! ```
//!
//! ### Deduplication Impact
//!
//! When deduplication detects a duplicate block:
//! - **No physical write**: Block is not written to disk
//! - **Offset reuse**: BlockInfo references the existing block's offset
//! - **Space savings**: Multiple logical blocks share one physical block
//! - **Transparency**: Readers cannot distinguish between deduplicated and unique blocks
//!
//! Example with deduplication:
//!
//! ```text
//! Logical Blocks: [A, B, A, C, B]
//! Physical Blocks: [A, B, C]
//!                   ↑  ↑     ↑
//!                   │  │     └─ Block 3 (unique)
//!                   │  └─ Block 1 (unique)
//!                   └─ Block 0 (unique)
//!
//! BlockInfo for logical block 2: offset = offset_of(A), length = len(A)
//! BlockInfo for logical block 4: offset = offset_of(B), length = len(B)
//! ```
//!
//! ## Buffer Management
//!
//! This module does not perform explicit buffer management. All buffers are:
//!
//! - **Caller-allocated**: Input chunks are provided by caller
//! - **Temporary allocations**: Compression/encryption output is allocated, then consumed
//! - **No pooling**: Each operation allocates fresh buffers (GC handles reclamation)
//!
//! For high-performance scenarios, callers should consider:
//! - Reusing chunk buffers across iterations
//! - Using buffer pools for compression output (requires refactoring)
//! - Batch writes to amortize allocation overhead
//!
//! ## Flush Behavior
//!
//! Functions in this module do NOT flush data to disk. Flushing is the caller's
//! responsibility and typically occurs:
//!
//! - After writing all blocks and indices (in [`pack_snapshot`](crate::ops::pack::pack_snapshot))
//! - Before closing the output file
//! - Never during block writing (to maximize write batching)
//!
//! This design allows the OS to batch writes for optimal I/O performance.
//!
//! # Error Handling and Recovery
//!
//! ## Error Categories
//!
//! Write operations can fail for several reasons:
//!
//! ### I/O Errors
//!
//! - **Disk full**: No space for compressed block (`ENOSPC`)
//! - **Permission denied**: Writer lacks write permission (`EACCES`)
//! - **Device error**: Hardware failure, I/O timeout (`EIO`)
//!
//! These surface as `StrataError::Io` wrapping the underlying `std::io::Error`.
//!
//! ### Compression Errors
//!
//! - **Compression failure**: Compressor returns error (rare, usually indicates bug)
//! - **Incompressible data**: Not an error; stored with expansion
//!
//! These surface as `StrataError::Compression`.
//!
//! ### Encryption Errors
//!
//! - **Cipher initialization failure**: Invalid state (should not occur in practice)
//! - **Encryption failure**: Crypto operation fails (indicates library bug)
//!
//! These surface as `StrataError::Encryption`.
//!
//! ## Error Recovery
//!
//! Write operations provide **no automatic recovery**. On error:
//!
//! - **Function returns immediately**: No cleanup or rollback
//! - **File state undefined**: Partial data may be written
//! - **Caller responsibility**: Must handle error and clean up
//!
//! Typical error handling pattern in pack operations:
//!
//! ```text
//! match write_block(...) {
//!     Ok(info) => {
//!         // Success: Add info to index, continue
//!     }
//!     Err(e) => {
//!         // Failure: Log error, delete partial output file, return error to caller
//!         std::fs::remove_file(output)?;
//!         return Err(e);
//!     }
//! }
//! ```
//!
//! ## Partial Write Handling
//!
//! The underlying `Write::write_all` method ensures atomic writes of complete blocks:
//!
//! - **Success**: Entire block written, offset updated
//! - **Failure**: Partial write may occur, but error is returned
//! - **No retry**: Caller must handle retries if desired
//!
//! # Performance Characteristics
//!
//! ## Write Throughput
//!
//! Block write performance is dominated by compression:
//!
//! - **LZ4**: ~2 GB/s (minimal overhead)
//! - **Zstd level 3**: ~200-500 MB/s (depends on data)
//! - **Encryption**: ~1-2 GB/s (hardware AES-NI)
//! - **SHA-256 hashing**: ~500 MB/s (for deduplication)
//!
//! Typical bottleneck: Compression CPU time.
//!
//! ## Deduplication Overhead
//!
//! SHA-256 hashing adds ~10-20% overhead to write operations:
//!
//! - **Hash computation**: ~500 MB/s throughput (crypto library optimized)
//! - **HashMap lookup**: O(1) average, ~50-100 ns per lookup
//! - **Memory usage**: ~48 bytes per unique block
//!
//! For datasets with <10% duplication, deduplication overhead may exceed savings.
//! Consider disabling dedup for unique data.
//!
//! ## Zero Block Detection
//!
//! [`is_zero_chunk`] uses SIMD-optimized comparison on modern CPUs:
//!
//! - **Throughput**: ~10-20 GB/s (memory bandwidth limited)
//! - **Overhead**: Negligible (~5-10 cycles per 64-byte cache line)
//!
//! Zero detection is always worth enabling for sparse data.
//!
//! # Memory Usage
//!
//! Per-block memory allocation:
//!
//! - **Input chunk**: Caller-provided (typically 64 KiB)
//! - **Compression output**: ~1.5× chunk size worst case (incompressible data)
//! - **Encryption output**: compression_size + 28 bytes (AES-GCM overhead)
//! - **Dedup hash**: 32 bytes (SHA-256 digest)
//!
//! Total temporary allocation per write: ~100-150 KiB (released immediately after write).
//!
//! # Examples
//!
//! See individual function documentation for usage examples.
//!
//! # Future Enhancements
//!
//! Potential improvements to write operations:
//!
//! - **Buffer pooling**: Reuse compression/encryption buffers to reduce allocation overhead
//! - **Async I/O**: Use `tokio` or `io_uring` for overlapped writes
//! - **Parallel writes**: Write multiple blocks concurrently (requires coordination)
//! - **Write-ahead logging**: Enable atomic commits for crash safety

use std::collections::HashMap;
use std::io::Write;
use strata_common::Result;

use crate::algo::compression::Compressor;
use crate::algo::encryption::Encryptor;
use crate::format::index::BlockInfo;

/// Writes a compressed and optionally encrypted block to the output stream.
///
/// This function implements the complete block transformation pipeline: compression,
/// optional encryption, checksum computation, deduplication, and physical write.
/// It returns a `BlockInfo` descriptor suitable for inclusion in an index page.
///
/// # Transformation Pipeline
///
/// 1. **Compression**: Compress raw chunk using provided compressor (LZ4 or Zstd)
/// 2. **Encryption** (optional): Encrypt compressed data with AES-256-GCM using block_idx as nonce
/// 3. **Checksum**: Compute CRC32 of final data for integrity verification
/// 4. **Deduplication** (optional, not for encrypted):
///    - Compute SHA-256 hash of final data
///    - Check dedup_map for existing block with same hash
///    - If found: Reuse existing offset, skip write
///    - If new: Write block, record offset in dedup_map
/// 5. **Write**: Append final data to output at current_offset
/// 6. **Metadata**: Create and return BlockInfo with offset, length, checksum
///
/// # Parameters
///
/// - `out`: Output writer implementing `Write` trait
///   - Typically a `File` or `BufWriter<File>`
///   - Must support `write_all` for atomic block writes
///
/// - `chunk`: Uncompressed chunk data (raw bytes)
///   - Typical size: 16 KiB - 256 KiB (configurable)
///   - Must not be empty (undefined behavior for zero-length chunks)
///
/// - `block_idx`: Global block index (zero-based)
///   - Used as encryption nonce (must be unique per snapshot)
///   - Monotonically increases across all streams
///   - Must not reuse indices within same encrypted snapshot (breaks security)
///
/// - `current_offset`: Mutable reference to current physical file offset
///   - Updated after successful write: `*current_offset += bytes_written`
///   - Not updated on error (file state undefined)
///   - Not updated for deduplicated blocks (reuses existing offset)
///
/// - `dedup_map`: Optional deduplication hash table
///   - `Some(&mut map)`: Enable dedup, use this map
///   - `None`: Disable dedup, always write
///   - Ignored if `encryptor.is_some()` (encryption prevents dedup)
///   - Maps SHA-256 hash → physical offset of first occurrence
///
/// - `compressor`: Compression algorithm implementation
///   - Typically `Lz4Compressor` or `ZstdCompressor`
///   - Must implement [`Compressor`](crate::algo::compression::Compressor) trait
///
/// - `encryptor`: Optional encryption implementation
///   - `Some(enc)`: Encrypt compressed data with AES-256-GCM
///   - `None`: Store compressed data unencrypted
///   - Must implement [`Encryptor`](crate::algo::encryption::Encryptor) trait
///
/// # Returns
///
/// - `Ok(BlockInfo)`: Block written successfully, metadata returned
///   - `offset`: Physical byte offset where block starts
///   - `length`: Compressed (and encrypted) size in bytes
///   - `logical_len`: Original uncompressed size
///   - `checksum`: CRC32 of final data (compressed + encrypted)
///
/// - `Err(StrataError::Io)`: I/O error during write
///   - Disk full, permission denied, device error
///   - File state undefined (partial write may have occurred)
///
/// - `Err(StrataError::Compression)`: Compression failed
///   - Rare; usually indicates library bug or corrupted input
///
/// - `Err(StrataError::Encryption)`: Encryption failed
///   - Rare; usually indicates crypto library bug
///
/// # Examples
///
/// ## Basic Usage (No Encryption, No Dedup)
///
/// ```no_run
/// use strata_core::ops::write::write_block;
/// use strata_core::algo::compression::Lz4Compressor;
/// use std::fs::File;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut out = File::create("output.st")?;
/// let mut offset = 512u64; // After header
/// let chunk = vec![0x42; 65536]; // 64 KiB of data
/// let compressor = Lz4Compressor::new();
///
/// let info = write_block(
///     &mut out,
///     &chunk,
///     0,              // block_idx
///     &mut offset,
///     None,           // No dedup
///     &compressor,
///     None,           // No encryption
/// )?;
///
/// println!("Block written at offset {}, size {}", info.offset, info.length);
/// # Ok(())
/// # }
/// ```
///
/// ## With Deduplication
///
/// ```no_run
/// use strata_core::ops::write::write_block;
/// use strata_core::algo::compression::Lz4Compressor;
/// use std::collections::HashMap;
/// use std::fs::File;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut out = File::create("output.st")?;
/// let mut offset = 512u64;
/// let mut dedup_map: HashMap<[u8; 32], u64> = HashMap::new();
/// let compressor = Lz4Compressor::new();
///
/// // Write first block
/// let chunk1 = vec![0xAA; 65536];
/// let info1 = write_block(
///     &mut out,
///     &chunk1,
///     0,
///     &mut offset,
///     Some(&mut dedup_map),
///     &compressor,
///     None,
/// )?;
/// println!("Block 0: offset={}, written", info1.offset);
///
/// // Write duplicate block (same content)
/// let chunk2 = vec![0xAA; 65536];
/// let info2 = write_block(
///     &mut out,
///     &chunk2,
///     1,
///     &mut offset,
///     Some(&mut dedup_map),
///     &compressor,
///     None,
/// )?;
/// println!("Block 1: offset={}, deduplicated (no write)", info2.offset);
/// assert_eq!(info1.offset, info2.offset); // Same offset, block reused
/// # Ok(())
/// # }
/// ```
///
/// ## With Encryption
///
/// ```no_run
/// use strata_core::ops::write::write_block;
/// use strata_core::algo::compression::Lz4Compressor;
/// use strata_core::algo::encryption::AesGcmEncryptor;
/// use strata_common::crypto::KeyDerivationParams;
/// use std::fs::File;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut out = File::create("output.st")?;
/// let mut offset = 512u64;
/// let compressor = Lz4Compressor::new();
///
/// // Initialize encryptor
/// let params = KeyDerivationParams::default();
/// let encryptor = AesGcmEncryptor::new(
///     b"strong_password",
///     &params.salt,
///     params.iterations,
/// );
///
/// let chunk = vec![0x42; 65536];
/// let info = write_block(
///     &mut out,
///     &chunk,
///     0,
///     &mut offset,
///     None,           // Dedup disabled (encryption prevents it)
///     &compressor,
///     Some(&encryptor),
/// )?;
///
/// println!("Encrypted block: offset={}, length={}", info.offset, info.length);
/// # Ok(())
/// # }
/// ```
///
/// # Performance
///
/// - **Compression**: Dominates runtime (~2 GB/s LZ4, ~500 MB/s Zstd)
/// - **Encryption**: ~1-2 GB/s (hardware AES-NI)
/// - **Hashing**: ~500 MB/s (SHA-256 for dedup)
/// - **I/O**: Typically not bottleneck (buffered writes, ~3 GB/s sequential)
///
/// # Deduplication Effectiveness
///
/// Deduplication is most effective when:
/// - **Fixed-size blocks**: Same content → same boundaries → same hash
/// - **Unencrypted**: Encryption produces unique ciphertext per block (different nonces)
/// - **Redundant data**: Duplicate files, repeated patterns, copy-on-write filesystems
///
/// Deduplication is ineffective when:
/// - **Content-defined chunking**: Small shifts cause different boundaries
/// - **Compressed input**: Pre-compressed data has low redundancy
/// - **Unique data**: No duplicate blocks to detect
///
/// # Security Considerations
///
/// ## Block Index as Nonce
///
/// When encrypting, `block_idx` is used as part of the AES-GCM nonce. **CRITICAL**:
/// - Never reuse `block_idx` values within the same encrypted snapshot
/// - Nonce reuse breaks AES-GCM security (allows plaintext recovery)
/// - Each logical block must have a unique index
///
/// ## Deduplication and Encryption
///
/// Deduplication is automatically disabled when encrypting because:
/// - Each block has a unique nonce → unique ciphertext
/// - SHA-256(ciphertext1) ≠ SHA-256(ciphertext2) even if plaintext is identical
/// - Attempting dedup with encryption wastes CPU (hashing) without space savings
///
/// # Thread Safety
///
/// This function is **not thread-safe** with respect to the output writer:
/// - Concurrent calls with the same `out` writer will interleave writes (corruption)
/// - Concurrent calls with different writers to the same file will corrupt file
///
/// For parallel writing, use separate output files or implement external synchronization.
///
/// The dedup_map must also be externally synchronized for concurrent access.
pub fn write_block<W: Write>(
    out: &mut W,
    chunk: &[u8],
    block_idx: u64,
    current_offset: &mut u64,
    dedup_map: Option<&mut HashMap<[u8; 32], u64>>,
    compressor: &dyn Compressor,
    encryptor: Option<&dyn Encryptor>,
) -> Result<BlockInfo> {
    use sha2::Digest;

    // Compress the chunk
    let compressed = compressor.compress(chunk)?;

    // Encrypt if requested
    let final_data = if let Some(enc) = encryptor {
        enc.encrypt(&compressed, block_idx)?
    } else {
        compressed
    };

    let checksum = crc32fast::hash(&final_data);
    let chunk_len = chunk.len() as u32;

    // Handle deduplication (only if not encrypting)
    let offset = if encryptor.is_some() {
        // No dedup for encrypted data
        let off = *current_offset;
        out.write_all(&final_data)?;
        *current_offset += final_data.len() as u64;
        off
    } else if let Some(map) = dedup_map {
        // Try to deduplicate
        let hash = sha2::Sha256::digest(&final_data);
        let hash_key: [u8; 32] = hash.into();

        if let Some(&existing_offset) = map.get(&hash_key) {
            // Block already exists, reuse it
            existing_offset
        } else {
            // New block, write it and record in map
            let off = *current_offset;
            map.insert(hash_key, off);
            out.write_all(&final_data)?;
            *current_offset += final_data.len() as u64;
            off
        }
    } else {
        // No dedup, just write
        let off = *current_offset;
        out.write_all(&final_data)?;
        *current_offset += final_data.len() as u64;
        off
    };

    Ok(BlockInfo {
        offset,
        length: final_data.len() as u32,
        logical_len: chunk_len,
        checksum,
    })
}

/// Creates a zero-block descriptor without writing data to disk.
///
/// Zero blocks (all-zero chunks) are a special case optimized for space efficiency.
/// Instead of compressing and storing zeros, we create a metadata-only descriptor
/// that signals to the reader to return zeros without performing any I/O.
///
/// # Sparse Data Optimization
///
/// Many VM disk images and memory dumps contain large regions of zeros:
/// - **Unallocated disk space**: File systems often zero-initialize blocks
/// - **Memory pages**: Unused or zero-initialized memory
/// - **Sparse files**: Holes in sparse file systems
///
/// Storing these zeros (even compressed) wastes space:
/// - **LZ4-compressed zeros**: ~100 bytes per 64 KiB block (~0.15% of original)
/// - **Uncompressed zeros**: 64 KiB per block (100%)
/// - **Metadata-only**: 20 bytes per block (~0.03%)
///
/// The metadata approach saves 99.97% of space for zero blocks.
///
/// # Descriptor Format
///
/// Zero blocks are identified by a special BlockInfo signature:
/// - `offset = 0`: Invalid physical offset (data region starts at ≥512)
/// - `length = 0`: No physical storage
/// - `logical_len = N`: Original zero block size in bytes
/// - `checksum = 0`: No checksum needed (zeros are deterministic)
///
/// Readers recognize this pattern and synthesize zeros without I/O.
///
/// # Parameters
///
/// - `logical_len`: Size of the zero block in bytes
///   - Typically matches block_size (e.g., 65536 for 64 KiB blocks)
///   - Can vary with content-defined chunking
///   - Must be > 0 (zero-length blocks are invalid)
///
/// # Returns
///
/// `BlockInfo` descriptor with zero-block semantics:
/// - `offset = 0`
/// - `length = 0`
/// - `logical_len = logical_len`
/// - `checksum = 0`
///
/// # Examples
///
/// ## Detecting and Creating Zero Blocks
///
/// ```
/// use strata_core::ops::write::{is_zero_chunk, create_zero_block};
/// use strata_core::format::index::BlockInfo;
///
/// let chunk = vec![0u8; 65536]; // 64 KiB of zeros
///
/// if is_zero_chunk(&chunk) {
///     let info = create_zero_block(chunk.len() as u32);
///     assert_eq!(info.offset, 0);
///     assert_eq!(info.length, 0);
///     assert_eq!(info.logical_len, 65536);
///     println!("Zero block: No storage required!");
/// }
/// ```
///
/// ## Usage in Packing Loop
///
/// ```no_run
/// # use strata_core::ops::write::{is_zero_chunk, create_zero_block, write_block};
/// # use strata_core::algo::compression::Lz4Compressor;
/// # use std::fs::File;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let mut out = File::create("output.st")?;
/// # let mut offset = 512u64;
/// # let compressor = Lz4Compressor::new();
/// # let chunks: Vec<Vec<u8>> = vec![];
/// for (idx, chunk) in chunks.iter().enumerate() {
///     let info = if is_zero_chunk(chunk) {
///         // Optimize: No compression, no write
///         create_zero_block(chunk.len() as u32)
///     } else {
///         // Normal path: Compress and write
///         write_block(&mut out, chunk, idx as u64, &mut offset, None, &compressor, None)?
///     };
///     // Add info to index page...
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Performance
///
/// - **Time complexity**: O(1) (no I/O, no computation)
/// - **Space complexity**: O(1) (fixed-size struct)
/// - **Typical savings**: 99.97% vs. compressed zeros
///
/// # Reader Behavior
///
/// When a reader encounters a zero block (offset=0, length=0):
/// 1. Recognize zero-block pattern from metadata
/// 2. Allocate buffer of size `logical_len`
/// 3. Fill buffer with zeros (optimized memset)
/// 4. Return buffer to caller
///
/// No decompression, decryption, or checksum verification is performed.
///
/// # Interaction with Deduplication
///
/// Zero blocks do not participate in deduplication:
/// - They are never written to disk → no physical offset → no dedup entry
/// - Each zero block gets its own metadata descriptor
/// - This is fine: Metadata is cheap (20 bytes), and all zero blocks have same content
///
/// # Interaction with Encryption
///
/// Zero blocks work correctly with encryption:
/// - They are detected **before** compression/encryption
/// - Encrypted snapshots still use zero-block optimization
/// - Readers synthesize zeros without decryption
///
/// This is safe because zeros are public information (no confidentiality lost).
///
/// # Validation
///
/// **IMPORTANT**: This function does NOT validate that the original chunk was actually
/// all zeros. The caller is responsible for calling [`is_zero_chunk`] first.
///
/// If a non-zero chunk is incorrectly marked as a zero block, readers will return
/// zeros instead of the original data (silent data corruption).
pub fn create_zero_block(logical_len: u32) -> BlockInfo {
    BlockInfo {
        offset: 0,
        length: 0,
        logical_len,
        checksum: 0,
    }
}

/// Checks if a chunk consists entirely of zero bytes.
///
/// This function efficiently detects all-zero chunks to enable sparse block optimization.
/// Zero chunks are common in VM images (unallocated space), memory dumps (zero-initialized
/// pages), and sparse files.
///
/// # Algorithm
///
/// Uses Rust's iterator `all()` combinator, which:
/// - Short-circuits on first non-zero byte (early exit)
/// - Compiles to SIMD instructions on modern CPUs (autovectorization)
/// - Typically processes 16-32 bytes per instruction (AVX2/AVX-512)
///
/// # Parameters
///
/// - `chunk`: Byte slice to check
///   - Empty slices return `true` (vacuous truth)
///   - Typical size: 16 KiB - 256 KiB (configurable block size)
///
/// # Returns
///
/// - `true`: All bytes are zero (sparse block, use [`create_zero_block`])
/// - `false`: At least one non-zero byte (normal block, compress and write)
///
/// # Performance
///
/// Modern CPUs with SIMD support achieve excellent throughput:
///
/// - **SIMD-optimized**: ~10-20 GB/s (memory bandwidth limited)
/// - **Scalar fallback**: ~1-2 GB/s (without SIMD)
/// - **Typical overhead**: <1% of total packing time
///
/// The check is always worth performing given the massive space savings for zero blocks.
///
/// # Examples
///
/// ## Basic Usage
///
/// ```
/// use strata_core::ops::write::is_zero_chunk;
///
/// let zeros = vec![0u8; 65536];
/// assert!(is_zero_chunk(&zeros));
///
/// let data = vec![0u8, 1u8, 0u8];
/// assert!(!is_zero_chunk(&data));
///
/// let empty: &[u8] = &[];
/// assert!(is_zero_chunk(empty)); // Empty is considered "all zeros"
/// ```
///
/// ## Packing Loop Integration
///
/// ```no_run
/// # use strata_core::ops::write::{is_zero_chunk, create_zero_block, write_block};
/// # use strata_core::algo::compression::Lz4Compressor;
/// # use strata_core::format::index::BlockInfo;
/// # use std::fs::File;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let mut out = File::create("output.st")?;
/// # let mut offset = 512u64;
/// # let compressor = Lz4Compressor::new();
/// # let mut index_blocks = Vec::new();
/// # let chunks: Vec<Vec<u8>> = vec![];
/// for (idx, chunk) in chunks.iter().enumerate() {
///     let info = if is_zero_chunk(chunk) {
///         // Fast path: No compression, no write, just metadata
///         create_zero_block(chunk.len() as u32)
///     } else {
///         // Slow path: Compress, write, create metadata
///         write_block(&mut out, chunk, idx as u64, &mut offset, None, &compressor, None)?
///     };
///     index_blocks.push(info);
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Benchmarking Zero Detection
///
/// ```
/// use strata_core::ops::write::is_zero_chunk;
/// use std::time::Instant;
///
/// let chunk = vec![0u8; 64 * 1024 * 1024]; // 64 MiB
/// let start = Instant::now();
///
/// for _ in 0..100 {
///     let _ = is_zero_chunk(&chunk);
/// }
///
/// let elapsed = start.elapsed();
/// let throughput = (64.0 * 100.0) / elapsed.as_secs_f64(); // MB/s
/// println!("Zero detection: {:.1} GB/s", throughput / 1024.0);
/// ```
///
/// # SIMD Optimization
///
/// On x86-64 with AVX2, the compiler typically generates code like:
///
/// ```text
/// vpxor    ymm0, ymm0, ymm0    ; Zero register
/// loop:
///   vmovdqu  ymm1, [rsi]        ; Load 32 bytes
///   vpcmpeqb ymm2, ymm1, ymm0   ; Compare with zero
///   vpmovmskb eax, ymm2         ; Extract comparison mask
///   cmp      eax, 0xFFFFFFFF    ; All zeros?
///   jne      found_nonzero      ; Early exit if not
///   add      rsi, 32            ; Advance pointer
///   loop
/// ```
///
/// This processes 32 bytes per iteration (~1-2 cycles on modern CPUs).
///
/// # Edge Cases
///
/// - **Empty chunks**: Return `true` (vacuous truth, no non-zero bytes)
/// - **Single byte**: Works correctly, no special handling needed
/// - **Unaligned chunks**: SIMD code handles unaligned loads transparently
///
/// # Alternative Implementations
///
/// Other possible implementations (not currently used):
///
/// 1. **Manual SIMD**: Use `std::arch` for explicit SIMD (faster but less portable)
/// 2. **Chunked comparison**: Process in 8-byte chunks with `u64` casts (faster scalar)
/// 3. **Bitmap scan**: Use CPU's `bsf`/`tzcnt` to skip zero regions (complex)
///
/// Current implementation relies on compiler autovectorization, which works well
/// in practice and maintains portability.
///
/// # Correctness
///
/// This function is pure and infallible:
/// - No side effects (read-only operation)
/// - No panics (iterator `all()` is safe for all inputs)
/// - No undefined behavior (all byte patterns are valid)
pub fn is_zero_chunk(chunk: &[u8]) -> bool {
    chunk.iter().all(|&b| b == 0)
}
