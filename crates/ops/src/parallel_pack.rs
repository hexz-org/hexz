//! Parallel packing pipeline for multi-threaded compression.
//!
//! This module implements a producer-consumer architecture where:
//! - Main thread reads and chunks input
//! - Worker threads compress chunks in parallel
//! - Single dedup thread writes unique blocks
//!
//! # Architecture
//!
//! ```text
//!                     ┌─→ Worker 1: Compress + Hash ─┐
//!                     │                                │
//! Main: Read chunks ──┼─→ Worker 2: Compress + Hash ──┼─→ Dedup + Write
//!                     │                                │   (sequential)
//!                     └─→ Worker N: Compress + Hash ─┘
//!
//! Parallel (N cores)                                   Sequential
//! ```
//!
//! This design maximizes CPU utilization while keeping dedup single-threaded
//! to avoid hash table locking overhead.

use crossbeam::channel::{Receiver, Sender, bounded};
use std::sync::Arc;
use std::thread;

use hexz_common::{Error, Result};
use hexz_core::algo::compression::Compressor;

/// Maximum chunks in flight to control memory usage
const MAX_CHUNKS_IN_FLIGHT: usize = 1000;

/// Raw chunk data from input
#[derive(Debug, Clone)]
pub struct RawChunk {
    /// Chunk data (uncompressed)
    pub data: Vec<u8>,
    /// Logical offset in the stream
    pub logical_offset: u64,
}

/// Compressed chunk with hash for deduplication
#[derive(Debug)]
pub struct CompressedChunk {
    /// Compressed data
    pub compressed: Vec<u8>,
    /// BLAKE3 hash of original data (for dedup)
    pub hash: [u8; 32],
    /// Logical offset in the stream
    pub logical_offset: u64,
    /// Original uncompressed size
    pub original_size: usize,
}

/// Configuration for parallel packing
#[derive(Debug, Clone)]
pub struct ParallelPackConfig {
    /// Number of worker threads
    pub num_workers: usize,
    /// Maximum chunks in flight (controls memory)
    pub max_chunks_in_flight: usize,
}

impl Default for ParallelPackConfig {
    fn default() -> Self {
        Self {
            num_workers: num_cpus::get(),
            max_chunks_in_flight: MAX_CHUNKS_IN_FLIGHT,
        }
    }
}

/// Compress worker: receives raw chunks, compresses them, computes hashes
fn compress_worker(
    rx_chunks: &Receiver<RawChunk>,
    tx_compressed: &Sender<CompressedChunk>,
    compressor: &(dyn Compressor + Send + Sync),
) -> Result<()> {
    for chunk in rx_chunks {
        // Compress the chunk
        let compressed = compressor.compress(&chunk.data)?;

        // Hash original data for deduplication
        let hash = blake3::hash(&chunk.data);

        // Send to dedup thread
        tx_compressed
            .send(CompressedChunk {
                compressed,
                hash: hash.into(),
                logical_offset: chunk.logical_offset,
                original_size: chunk.data.len(),
            })
            .map_err(|_| {
                Error::Io(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "Compressed chunk channel send failed",
                ))
            })?;
    }

    Ok(())
}

/// Process chunks in parallel with multiple worker threads
///
/// # Arguments
///
/// * `chunks` - Vector of raw chunks to process
/// * `compressor` - Compressor to use (must be Send + Sync)
/// * `config` - Parallel processing configuration
/// * `writer` - Callback for each compressed chunk (handles dedup + write)
///
/// # Returns
///
/// Ok(()) on success, or Error if any worker fails
pub fn process_chunks_parallel<W>(
    chunks: Vec<RawChunk>,
    compressor: Box<dyn Compressor + Send + Sync>,
    config: &ParallelPackConfig,
    mut writer: W,
) -> Result<()>
where
    W: FnMut(CompressedChunk) -> Result<()>,
{
    // Can't parallelize if we have fewer chunks than workers
    if chunks.is_empty() {
        return Ok(());
    }

    let compressor = Arc::new(compressor);

    // Create bounded channels to control memory usage
    let (tx_chunks, rx_chunks) = bounded::<RawChunk>(config.max_chunks_in_flight);
    let (tx_compressed, rx_compressed) = bounded::<CompressedChunk>(config.max_chunks_in_flight);

    // Spawn compression worker threads
    let workers: Vec<_> = (0..config.num_workers)
        .map(|_| {
            let rx = rx_chunks.clone();
            let tx = tx_compressed.clone();
            let comp = compressor.clone();

            thread::spawn(move || compress_worker(&rx, &tx, comp.as_ref().as_ref()))
        })
        .collect();

    // Spawn feeder thread to send chunks to workers
    let feeder = {
        let tx = tx_chunks.clone();
        thread::spawn(move || {
            for chunk in chunks {
                if tx.send(chunk).is_err() {
                    // Channel closed, workers are done
                    break;
                }
            }
            // Drop tx to signal workers we're done
        })
    };

    // Drop our sender handles so workers will finish when feeder is done
    drop(tx_chunks);
    drop(tx_compressed);

    // Main thread: receive compressed chunks and process (dedup + write)
    // This must be sequential for dedup correctness
    for compressed in rx_compressed {
        writer(compressed)?;
    }

    // Wait for all workers to finish and check for errors
    for worker in workers {
        worker
            .join()
            .map_err(|_| Error::Io(std::io::Error::other("Worker thread panicked")))??;
    }

    feeder
        .join()
        .map_err(|_| Error::Io(std::io::Error::other("Feeder thread panicked")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hexz_core::algo::compression::lz4::Lz4Compressor;

    #[test]
    fn test_parallel_pack_empty_chunks() {
        let chunks = vec![];
        let compressor = Box::new(Lz4Compressor::new());
        let config = ParallelPackConfig::default();

        let result = process_chunks_parallel(chunks, compressor, &config, |_| Ok(()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_parallel_pack_single_chunk() {
        let chunks = vec![RawChunk {
            data: vec![1, 2, 3, 4, 5],
            logical_offset: 0,
        }];

        let compressor = Box::new(Lz4Compressor::new());
        let config = ParallelPackConfig {
            num_workers: 2,
            max_chunks_in_flight: 10,
        };

        let mut received = Vec::new();
        let result = process_chunks_parallel(chunks, compressor, &config, |chunk| {
            received.push(chunk.original_size);
            Ok(())
        });

        assert!(result.is_ok());
        assert_eq!(received.len(), 1);
        assert_eq!(received[0], 5);
    }

    #[test]
    fn test_parallel_pack_multiple_chunks() {
        let chunks: Vec<_> = (0..100)
            .map(|i| RawChunk {
                data: vec![i as u8; 1000],
                logical_offset: i * 1000,
            })
            .collect();

        let compressor = Box::new(Lz4Compressor::new());
        let config = ParallelPackConfig {
            num_workers: 4,
            max_chunks_in_flight: 20,
        };

        let mut count = 0;
        let result = process_chunks_parallel(chunks, compressor, &config, |_chunk| {
            count += 1;
            Ok(())
        });

        assert!(result.is_ok());
        assert_eq!(count, 100);
    }
}
