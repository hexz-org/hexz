//! Data Prefetching Logic.
//!
//! This module implements the prefetching strategy, which predicts future
//! block accesses based on current read patterns. By proactively fetching
//! data into the cache before it is explicitly requested, the system can
//! significantly reduce read latency for sequential access workloads.

use std::sync::atomic::{AtomicU32, Ordering};

/// Manages prefetching state and logic.
///
/// This struct tracks the configured prefetch distance (window size).
/// It uses atomic operations to ensure thread safety, allowing the prefetch
/// window to be adjusted dynamically at runtime if needed.
#[derive(Debug)]
pub struct Prefetcher {
    /// The number of blocks to fetch ahead of the current request.
    window_size: AtomicU32,
}

impl Prefetcher {
    /// Initializes a new prefetcher with the specified lookahead window.
    ///
    /// # Arguments
    ///
    /// * `window_size` - The number of blocks to fetch ahead.
    ///
    /// # Returns
    ///
    /// Returns a new `Prefetcher` instance.
    pub fn new(window_size: u32) -> Self {
        Self {
            window_size: AtomicU32::new(window_size),
        }
    }

    /// Computes the list of block indices that should be prefetched.
    ///
    /// This method calculates the range of block indices that follow the
    /// currently accessed block, up to the configured window size.
    ///
    /// # Arguments
    ///
    /// * `current_block` - The index of the block currently being accessed.
    ///
    /// # Returns
    ///
    /// Returns a `Vec<u64>` containing the indices of blocks to prefetch.
    /// Returns an empty vector if prefetching is disabled (`window_size` is 0).
    pub fn get_prefetch_targets(&self, current_block: u64) -> Vec<u64> {
        let size = self.window_size.load(Ordering::Relaxed);
        if size == 0 {
            return Vec::new();
        }

        (1..=size as u64)
            .map(|offset| current_block + offset)
            .collect()
    }
}
