//! Reusable buffer pool for decompression output buffers.
//!
//! Eliminates per-block `Vec<u8>` allocations during decompression by pooling
//! and reusing buffers of common sizes. This is especially important for the
//! parallel decompression path where N threads hitting the global allocator
//! concurrently causes contention.
//!
//! # Design
//!
//! The pool is a simple `Mutex<Vec<Vec<u8>>>` stack. Buffers are checked out
//! for decompression and returned after the decompressed data has been copied
//! or converted to `Bytes`.
//!
//! When a buffer is needed for cache insertion (converting to `Bytes`), the
//! buffer is consumed and not returned to the pool. The pool naturally reaches
//! steady state as the block cache fills up and stops triggering new
//! decompressions.

use std::sync::Mutex;

/// A pool of reusable `Vec<u8>` buffers for decompression.
///
/// Thread-safe via internal `Mutex`. The pool stores buffers sorted by capacity
/// to enable efficient size-matched checkout.
pub struct BufferPool {
    /// Stack of available buffers, largest capacity last for O(1) pop.
    buffers: Mutex<Vec<Vec<u8>>>,
    /// Maximum number of buffers to retain in the pool.
    max_buffers: usize,
}

impl BufferPool {
    /// Creates a new buffer pool.
    ///
    /// # Parameters
    ///
    /// - `max_buffers`: Maximum number of idle buffers to retain.
    ///   Excess buffers returned via `checkin` are dropped.
    pub fn new(max_buffers: usize) -> Self {
        Self {
            buffers: Mutex::new(Vec::with_capacity(max_buffers)),
            max_buffers,
        }
    }

    /// Checks out a buffer with at least `capacity` bytes.
    ///
    /// Returns a pooled buffer if one of sufficient size is available,
    /// otherwise allocates a new one. The returned buffer has length 0
    /// but capacity >= `capacity`.
    pub fn checkout(&self, capacity: usize) -> Vec<u8> {
        if let Ok(mut pool) = self.buffers.lock() {
            // Find the first buffer with sufficient capacity (linear scan is
            // fine since max_buffers is small, typically 8-32).
            if let Some(idx) = pool.iter().position(|b| b.capacity() >= capacity) {
                let mut buf = pool.swap_remove(idx);
                buf.clear();
                return buf;
            }
        }
        Vec::with_capacity(capacity)
    }

    /// Returns a buffer to the pool for future reuse.
    ///
    /// If the pool is full, the buffer is dropped. Buffers are only worth
    /// pooling if they have meaningful capacity (the caller should not return
    /// tiny buffers).
    pub fn checkin(&self, buf: Vec<u8>) {
        if buf.capacity() == 0 {
            return;
        }
        if let Ok(mut pool) = self.buffers.lock() {
            if pool.len() < self.max_buffers {
                pool.push(buf);
            }
            // else: drop buf (pool is full)
        }
        // else: lock poisoned, just drop the buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkout_returns_sufficient_capacity() {
        let pool = BufferPool::new(4);
        let buf = pool.checkout(1024);
        assert!(buf.capacity() >= 1024);
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn checkin_and_reuse() {
        let pool = BufferPool::new(4);

        // Allocate and return a buffer
        let mut buf = pool.checkout(1024);
        buf.extend_from_slice(&[42u8; 512]);
        let cap = buf.capacity();
        pool.checkin(buf);

        // Should get the same buffer back (same capacity, cleared)
        let buf2 = pool.checkout(1024);
        assert_eq!(buf2.capacity(), cap);
        assert_eq!(buf2.len(), 0);
    }

    #[test]
    fn respects_max_buffers() {
        let pool = BufferPool::new(2);

        pool.checkin(Vec::with_capacity(1024));
        pool.checkin(Vec::with_capacity(1024));
        pool.checkin(Vec::with_capacity(1024)); // should be dropped

        let guard = pool.buffers.lock().unwrap();
        assert_eq!(guard.len(), 2);
    }

    #[test]
    fn checkout_allocates_when_empty() {
        let pool = BufferPool::new(4);
        let buf = pool.checkout(65536);
        assert!(buf.capacity() >= 65536);
    }

    #[test]
    fn checkout_skips_too_small_buffers() {
        let pool = BufferPool::new(4);
        pool.checkin(Vec::with_capacity(512));

        // Request larger than available
        let buf = pool.checkout(1024);
        assert!(buf.capacity() >= 1024);

        // The small buffer should still be in the pool
        let guard = pool.buffers.lock().unwrap();
        assert_eq!(guard.len(), 1);
    }
}
