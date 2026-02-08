//! Content-Defined Chunking (CDC) for deduplication analysis.
//!
//! This module implements FastCDC (Fast Content-Defined Chunking), a rolling-hash
//! algorithm that splits data into variable-sized chunks based on content rather
//! than fixed offsets. This enables efficient deduplication by detecting repeated
//! content even when it's shifted or surrounded by different data.
//!
//! # Algorithm
//!
//! FastCDC uses a rolling hash to identify "cut points" in the data stream:
//! 1. Compute a rolling hash over a sliding window
//! 2. When hash matches a pattern (e.g., low N bits are zero), mark a cut point
//! 3. Enforce min/avg/max chunk size constraints
//! 4. Hash each chunk for deduplication tracking
//!
//! # Usage
//!
//! CDC is used in two contexts:
//! - **Analysis**: `analyze_stream()` performs a dry-run to estimate deduplication ratio
//! - **Streaming**: `StreamChunker` yields chunks during actual snapshot creation
//!
//! # Performance
//!
//! - **Throughput**: ~500 MB/s (single-threaded)
//! - **Memory**: ~2MB buffer per stream
//! - **Chunk Size**: Typically 16KB-64KB (configurable)
//!
//! # Examples
//!
//! ```no_run
//! use strata_core::algo::dedup::cdc::{analyze_stream, StreamChunker};
//! use strata_core::algo::dedup::dcam::DedupeParams;
//! use std::fs::File;
//!
//! # fn main() -> std::io::Result<()> {
//! // Analyze deduplication potential
//! let file = File::open("data.bin")?;
//! let params = DedupeParams::default();
//! let stats = analyze_stream(file, &params)?;
//!
//! println!("Unique chunks: {}/{}", stats.unique_chunk_count, stats.chunk_count);
//! println!("Dedup ratio: {:.2}%",
//!          (1.0 - stats.unique_bytes as f64 / stats.total_bytes as f64) * 100.0);
//!
//! // Stream chunks for processing
//! let file = File::open("data.bin")?;
//! let chunker = StreamChunker::new(file, params);
//! for chunk in chunker {
//!     let data = chunk?;
//!     // Process chunk...
//! }
//! # Ok(())
//! # }
//! ```

use crate::algo::dedup::dcam::DedupeParams;
use std::collections::HashSet;
use std::io::{self, Read};

/// Statistics from a CDC deduplication analysis run.
///
/// Contains metrics about chunk distribution and deduplication effectiveness.
/// Used by the DCAM model to estimate entropy and compression ratio.
#[derive(Debug)]
pub struct CdcStats {
    /// Total bytes processed (may be 0 if not tracked during streaming).
    pub total_bytes: u64,

    /// Number of unique bytes after deduplication.
    /// This is the sum of sizes of all unique chunks.
    pub unique_bytes: u64,

    /// Total number of chunks identified by FastCDC.
    pub chunk_count: u64,

    /// Number of unique chunks (deduplicated count).
    pub unique_chunk_count: u64,
}

/// Streaming iterator that yields content-defined chunks from a reader.
///
/// This chunker reads data incrementally and applies FastCDC to split it into
/// variable-sized chunks. It maintains an internal buffer to handle partial reads
/// and chunk boundaries that span multiple `read()` calls.
///
/// # Buffering Strategy
///
/// - Buffer size is `2 * max_chunk_size` to avoid frequent reallocations
/// - Data is shifted when cursor advances beyond halfway point
/// - Refills happen when available data is less than max chunk size
///
/// # Examples
///
/// ```no_run
/// use strata_core::algo::dedup::cdc::StreamChunker;
/// use strata_core::algo::dedup::dcam::DedupeParams;
/// use std::fs::File;
///
/// # fn main() -> std::io::Result<()> {
/// let file = File::open("data.bin")?;
/// let params = DedupeParams::default();
/// let chunker = StreamChunker::new(file, params);
///
/// for chunk_result in chunker {
///     let chunk = chunk_result?;
///     println!("Chunk: {} bytes", chunk.len());
/// }
/// # Ok(())
/// # }
/// ```
pub struct StreamChunker<R> {
    /// Underlying data source.
    reader: R,

    /// Internal buffer (size = 2 * max_chunk_size).
    buffer: Vec<u8>,

    /// Current read position in buffer.
    cursor: usize,

    /// Number of valid bytes in buffer.
    filled: usize,

    /// Minimum chunk size (FastCDC parameter).
    min_size: usize,

    /// Average chunk size (1 << f, where f is the FastCDC bits parameter).
    avg_size: usize,

    /// Maximum chunk size (FastCDC parameter).
    max_size: usize,

    /// Whether EOF has been reached on the reader.
    eof: bool,
}

impl<R: Read> StreamChunker<R> {
    /// Creates a new streaming chunker with the specified deduplication parameters.
    ///
    /// # Parameters
    ///
    /// - `reader`: Data source implementing `Read`
    /// - `params`: FastCDC parameters (min/avg/max chunk sizes)
    ///
    /// # Returns
    ///
    /// A new chunker ready to yield chunks via iteration.
    ///
    /// # Buffer Size
    ///
    /// Allocates a buffer of `2 * params.z` (max chunk size) to handle streaming
    /// efficiently. For default settings (z=64KB), this is ~128KB per chunker.
    pub fn new(reader: R, params: DedupeParams) -> Self {
        // Buffer needs to be at least max_size.
        // We use 2 * max_size to allow for shifting and efficient reading.
        let buf_size = (params.z as usize).max(1024 * 1024) * 2;
        Self {
            reader,
            buffer: vec![0u8; buf_size],
            cursor: 0,
            filled: 0,
            min_size: params.m as usize,
            avg_size: 1 << params.f,
            max_size: params.z as usize,
            eof: false,
        }
    }

    /// Refills the internal buffer from the reader.
    ///
    /// This method:
    /// 1. Shifts unprocessed data to the start of the buffer
    /// 2. Reads more data to fill available space
    /// 3. Updates the EOF flag when reader is exhausted
    ///
    /// # Errors
    ///
    /// Returns I/O errors from the underlying reader.
    fn refill(&mut self) -> io::Result<()> {
        if self.cursor > 0 {
            // Shift remaining data to start
            self.buffer.copy_within(self.cursor..self.filled, 0);
            self.filled -= self.cursor;
            self.cursor = 0;
        }

        while self.filled < self.buffer.len() && !self.eof {
            let n = self.reader.read(&mut self.buffer[self.filled..])?;
            if n == 0 {
                self.eof = true;
            } else {
                self.filled += n;
                // If we have enough data to potentially find a chunk, we can stop reading for now
                if self.filled >= self.max_size {
                    break;
                }
            }
        }
        Ok(())
    }
}

impl<R: Read> Iterator for StreamChunker<R> {
    type Item = io::Result<Vec<u8>>;

    /// Yields the next content-defined chunk.
    ///
    /// This method:
    /// 1. Refills the buffer if needed
    /// 2. Runs FastCDC on available data to find cut point
    /// 3. Falls back to max chunk size if no cut point found
    /// 4. Returns the chunk as an owned `Vec<u8>`
    ///
    /// # Returns
    ///
    /// - `Some(Ok(chunk))` - Next chunk successfully extracted
    /// - `Some(Err(e))` - I/O error reading from underlying source
    /// - `None` - Stream exhausted (EOF reached)
    ///
    /// # Chunk Size Guarantees
    ///
    /// - Minimum: `params.m` (except possibly last chunk)
    /// - Maximum: `params.z` (always enforced)
    /// - Average: ~`1 << params.f` (statistical, depends on content)
    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.filled {
            if self.eof {
                return None;
            }
            if let Err(e) = self.refill() {
                return Some(Err(e));
            }
            if self.filled == 0 {
                return None;
            }
        }

        let available = self.filled - self.cursor;

        let chunk_len = if available < self.min_size {
            available
        } else {
            // Run FastCDC on the available window
            let data = &self.buffer[self.cursor..self.filled];
            let search_limit = std::cmp::min(data.len(), self.max_size);

            let chunker = fastcdc::v2020::FastCDC::new(
                &data[..search_limit],
                self.min_size as u32,
                self.avg_size as u32,
                self.max_size as u32,
            );

            if let Some(chunk) = chunker.into_iter().next() {
                chunk.length
            } else {
                // No cut point found
                if available >= self.max_size {
                    self.max_size
                } else if self.eof {
                    available
                } else if self.filled == self.buffer.len() {
                    self.max_size
                } else {
                    available
                }
            }
        };

        let start = self.cursor;
        self.cursor += chunk_len;
        Some(Ok(self.buffer[start..start + chunk_len].to_vec()))
    }
}

/// Performs a single-pass deduplication analysis on a data stream.
///
/// This function:
/// 1. Chunks the input using FastCDC
/// 2. Computes a hash (CRC32) for each chunk
/// 3. Tracks unique chunks in a hash set
/// 4. Returns statistics about chunk distribution
///
/// # Parameters
///
/// - `reader`: Data source to analyze
/// - `params`: FastCDC parameters (min/avg/max chunk sizes)
///
/// # Returns
///
/// `CdcStats` containing:
/// - Total chunks found
/// - Unique chunks (after deduplication)
/// - Total unique bytes
///
/// # Performance
///
/// - Memory: O(unique chunks) for hash set (~8 bytes per unique chunk)
/// - CPU: One pass through data (~500 MB/s single-threaded)
///
/// # Examples
///
/// ```no_run
/// use strata_core::algo::dedup::cdc::analyze_stream;
/// use strata_core::algo::dedup::dcam::DedupeParams;
/// use std::fs::File;
///
/// # fn main() -> std::io::Result<()> {
/// let file = File::open("dataset.bin")?;
/// let params = DedupeParams::default();
/// let stats = analyze_stream(file, &params)?;
///
/// let dedup_ratio = 1.0 - (stats.unique_bytes as f64 / stats.total_bytes as f64);
/// println!("Deduplication would save {:.1}% storage", dedup_ratio * 100.0);
/// # Ok(())
/// # }
/// ```
///
/// # Note
///
/// This is a "dry run" analysis that doesn't store chunks, only their hashes.
/// Use `StreamChunker` for actual chunk processing during snapshot creation.
pub fn analyze_stream<R: Read>(reader: R, params: &DedupeParams) -> io::Result<CdcStats> {
    let mut unique_bytes = 0;
    let mut chunk_count = 0;
    let mut unique_chunk_count = 0;
    let mut seen_chunks: HashSet<u64> = HashSet::new();

    // Use the streaming chunker for analysis too, to ensure consistency
    let chunker = StreamChunker::new(reader, *params);

    for chunk_res in chunker {
        let chunk = chunk_res?;
        let len = chunk.len() as u64;
        let hash = crc32fast::hash(&chunk) as u64;

        chunk_count += 1;
        if seen_chunks.insert(hash) {
            unique_bytes += len;
            unique_chunk_count += 1;
        }
    }

    Ok(CdcStats {
        total_bytes: 0,
        unique_bytes,
        chunk_count,
        unique_chunk_count,
    })
}
