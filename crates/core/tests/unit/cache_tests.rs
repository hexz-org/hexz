//! Unit tests for caching layers (LRU block cache).
//!
//! Tests verify cache hit/miss behavior, eviction policy, and thread safety.

use bytes::Bytes;
use std::sync::Arc;
use std::thread;
use strata_core::SnapshotStream;
use strata_core::cache::lru::BlockCache;

/// Test basic cache insertion and retrieval.
#[test]
fn test_cache_insert_and_get() {
    let cache = BlockCache::with_capacity(10);
    let data = Bytes::from(vec![1, 2, 3, 4]);

    cache.insert(SnapshotStream::Disk, 0, data.clone());

    let retrieved = cache.get(SnapshotStream::Disk, 0);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap(), data);
}

/// Test cache miss returns None.
#[test]
fn test_cache_miss() {
    let cache = BlockCache::with_capacity(10);

    let result = cache.get(SnapshotStream::Disk, 999);
    assert!(result.is_none());
}

/// Test cache eviction with LRU policy.
#[test]
fn test_cache_lru_eviction() {
    let cache = BlockCache::with_capacity(3);

    // Insert 3 blocks
    cache.insert(SnapshotStream::Disk, 0, Bytes::from(vec![0]));
    cache.insert(SnapshotStream::Disk, 1, Bytes::from(vec![1]));
    cache.insert(SnapshotStream::Disk, 2, Bytes::from(vec![2]));

    // All should be present
    assert!(cache.get(SnapshotStream::Disk, 0).is_some());
    assert!(cache.get(SnapshotStream::Disk, 1).is_some());
    assert!(cache.get(SnapshotStream::Disk, 2).is_some());

    // Insert a 4th block, should evict LRU (block 0)
    cache.insert(SnapshotStream::Disk, 3, Bytes::from(vec![3]));

    // Block 0 should be evicted
    assert!(cache.get(SnapshotStream::Disk, 0).is_none());
    assert!(cache.get(SnapshotStream::Disk, 1).is_some());
    assert!(cache.get(SnapshotStream::Disk, 2).is_some());
    assert!(cache.get(SnapshotStream::Disk, 3).is_some());
}

/// Test that accessing a block updates its LRU position.
#[test]
fn test_cache_lru_access_updates() {
    let cache = BlockCache::with_capacity(3);

    // Insert 3 blocks
    cache.insert(SnapshotStream::Disk, 0, Bytes::from(vec![0]));
    cache.insert(SnapshotStream::Disk, 1, Bytes::from(vec![1]));
    cache.insert(SnapshotStream::Disk, 2, Bytes::from(vec![2]));

    // Access block 0 to make it recently used
    let _ = cache.get(SnapshotStream::Disk, 0);

    // Insert new block, should evict block 1 (not 0)
    cache.insert(SnapshotStream::Disk, 3, Bytes::from(vec![3]));

    assert!(cache.get(SnapshotStream::Disk, 0).is_some());
    assert!(cache.get(SnapshotStream::Disk, 1).is_none()); // Evicted
    assert!(cache.get(SnapshotStream::Disk, 2).is_some());
    assert!(cache.get(SnapshotStream::Disk, 3).is_some());
}

/// Test separate caching for disk and memory streams.
#[test]
fn test_cache_separate_streams() {
    let cache = BlockCache::with_capacity(10);

    let disk_data = Bytes::from(vec![1, 2, 3]);
    let mem_data = Bytes::from(vec![4, 5, 6]);

    // Insert same block index for different streams
    cache.insert(SnapshotStream::Disk, 0, disk_data.clone());
    cache.insert(SnapshotStream::Memory, 0, mem_data.clone());

    // Both should be retrievable independently
    assert_eq!(cache.get(SnapshotStream::Disk, 0).unwrap(), disk_data);
    assert_eq!(cache.get(SnapshotStream::Memory, 0).unwrap(), mem_data);
}

/// Test cache with zero capacity (edge case).
#[test]
fn test_cache_zero_capacity() {
    let cache = BlockCache::with_capacity(0);

    cache.insert(SnapshotStream::Disk, 0, Bytes::from(vec![1]));

    // With zero capacity, nothing should be cached
    assert!(cache.get(SnapshotStream::Disk, 0).is_none());
}

/// Test cache with single entry capacity.
#[test]
fn test_cache_single_capacity() {
    let cache = BlockCache::with_capacity(1);

    cache.insert(SnapshotStream::Disk, 0, Bytes::from(vec![0]));
    assert!(cache.get(SnapshotStream::Disk, 0).is_some());

    cache.insert(SnapshotStream::Disk, 1, Bytes::from(vec![1]));
    assert!(cache.get(SnapshotStream::Disk, 0).is_none());
    assert!(cache.get(SnapshotStream::Disk, 1).is_some());
}

/// Test cache with large capacity.
#[test]
fn test_cache_large_capacity() {
    let cache = BlockCache::with_capacity(10000);

    // Insert many blocks
    for i in 0..1000 {
        cache.insert(SnapshotStream::Disk, i, Bytes::from(vec![i as u8]));
    }

    // All should be present (under capacity)
    for i in 0..1000 {
        assert!(cache.get(SnapshotStream::Disk, i).is_some());
    }
}

/// Test thread safety - concurrent reads.
#[test]
fn test_cache_concurrent_reads() {
    let cache = Arc::new(BlockCache::with_capacity(100));

    // Pre-populate cache
    for i in 0..10 {
        cache.insert(SnapshotStream::Disk, i, Bytes::from(vec![i as u8]));
    }

    let mut handles = vec![];

    // Spawn multiple reader threads
    for _ in 0..10 {
        let cache_clone = cache.clone();
        let handle = thread::spawn(move || {
            for i in 0..10 {
                let result = cache_clone.get(SnapshotStream::Disk, i);
                assert!(result.is_some());
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

/// Test thread safety - concurrent writes.
#[test]
fn test_cache_concurrent_writes() {
    let cache = Arc::new(BlockCache::with_capacity(100));
    let mut handles = vec![];

    // Spawn multiple writer threads
    for thread_id in 0..10 {
        let cache_clone = cache.clone();
        let handle = thread::spawn(move || {
            for i in 0..10 {
                let block_idx = thread_id * 10 + i;
                cache_clone.insert(
                    SnapshotStream::Disk,
                    block_idx,
                    Bytes::from(vec![block_idx as u8]),
                );
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all blocks were inserted
    for i in 0..100 {
        assert!(cache.get(SnapshotStream::Disk, i).is_some());
    }
}

/// Test thread safety - mixed reads and writes.
#[test]
fn test_cache_concurrent_mixed() {
    let cache = Arc::new(BlockCache::with_capacity(50));

    // Pre-populate
    for i in 0..10 {
        cache.insert(SnapshotStream::Disk, i, Bytes::from(vec![i as u8]));
    }

    let mut handles = vec![];

    // Spawn reader threads
    for _ in 0..5 {
        let cache_clone = cache.clone();
        let handle = thread::spawn(move || {
            for i in 0..10 {
                let _ = cache_clone.get(SnapshotStream::Disk, i);
            }
        });
        handles.push(handle);
    }

    // Spawn writer threads
    for thread_id in 0..5 {
        let cache_clone = cache.clone();
        let handle = thread::spawn(move || {
            for i in 10..20 {
                let block_idx = thread_id * 10 + i;
                cache_clone.insert(
                    SnapshotStream::Disk,
                    block_idx,
                    Bytes::from(vec![block_idx as u8]),
                );
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

/// Test cache behavior with duplicate insertions (overwrites).
#[test]
fn test_cache_overwrite() {
    let cache = BlockCache::with_capacity(10);

    cache.insert(SnapshotStream::Disk, 0, Bytes::from(vec![1, 2, 3]));
    let first = cache.get(SnapshotStream::Disk, 0).unwrap();
    assert_eq!(first, Bytes::from(vec![1, 2, 3]));

    // Overwrite with new data
    cache.insert(SnapshotStream::Disk, 0, Bytes::from(vec![4, 5, 6]));
    let second = cache.get(SnapshotStream::Disk, 0).unwrap();
    assert_eq!(second, Bytes::from(vec![4, 5, 6]));
}

/// Test cache with varying data sizes.
#[test]
fn test_cache_various_data_sizes() {
    let cache = BlockCache::with_capacity(100);

    // Small block
    cache.insert(SnapshotStream::Disk, 0, Bytes::from(vec![1; 128]));

    // Medium block
    cache.insert(SnapshotStream::Disk, 1, Bytes::from(vec![2; 4096]));

    // Large block
    cache.insert(SnapshotStream::Disk, 2, Bytes::from(vec![3; 65536]));

    assert_eq!(cache.get(SnapshotStream::Disk, 0).unwrap().len(), 128);
    assert_eq!(cache.get(SnapshotStream::Disk, 1).unwrap().len(), 4096);
    assert_eq!(cache.get(SnapshotStream::Disk, 2).unwrap().len(), 65536);
}

/// Test cache key generation for different streams and indices.
#[test]
fn test_cache_key_uniqueness() {
    let cache = BlockCache::with_capacity(10);

    // Insert blocks with same index but different streams
    cache.insert(SnapshotStream::Disk, 5, Bytes::from(vec![1]));
    cache.insert(SnapshotStream::Memory, 5, Bytes::from(vec![2]));

    // Insert blocks with different indices same stream
    cache.insert(SnapshotStream::Disk, 6, Bytes::from(vec![3]));
    cache.insert(SnapshotStream::Disk, 7, Bytes::from(vec![4]));

    // All should be independently retrievable
    assert_eq!(cache.get(SnapshotStream::Disk, 5).unwrap()[0], 1);
    assert_eq!(cache.get(SnapshotStream::Memory, 5).unwrap()[0], 2);
    assert_eq!(cache.get(SnapshotStream::Disk, 6).unwrap()[0], 3);
    assert_eq!(cache.get(SnapshotStream::Disk, 7).unwrap()[0], 4);
}
