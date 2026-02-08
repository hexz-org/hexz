//! Prefetching Strategy Performance Benchmarks.
//!
//! Evaluates the effectiveness of various prefetching strategies for AI workloads.
//! Prefetching is critical for hiding I/O latency during sequential and semi-sequential
//! access patterns common in ML training (e.g., reading consecutive mini-batches).

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::sync::Arc;
use strata_core::api::stratafile::SnapshotStream;

#[path = "common.rs"]
mod common;

/// Simulates prefetching with various window sizes.
///
/// Tests how different prefetch window sizes affect throughput during
/// sequential access. Larger windows reduce latency but increase memory
/// pressure and may prefetch data that won't be used.
fn bench_prefetch_window_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("Prefetch/WindowSize");

    let num_blocks = 1000;
    let block_size = 65536; // 64KB blocks
    let (_input, _output, snapshot) = common::create_dataset(num_blocks, block_size);
    let dataset = Arc::new(snapshot);
    let total_bytes = (num_blocks * block_size) as u64;

    // Test window sizes from 0 (no prefetch) to 32 blocks ahead
    for window_size in [0, 1, 2, 4, 8, 16, 32] {
        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(
            BenchmarkId::new("blocks", window_size),
            &window_size,
            |b, &window| {
                b.iter(|| {
                    // Simulate prefetch-ahead pattern
                    for i in 0..num_blocks {
                        let offset = (i * block_size) as u64;

                        // Read current block
                        let data = dataset
                            .read_at(SnapshotStream::Disk, offset, block_size)
                            .unwrap();
                        black_box(data);

                        // Simulate prefetch of future blocks
                        for j in 1..=window {
                            let prefetch_idx = i + j;
                            if prefetch_idx < num_blocks {
                                let prefetch_offset = (prefetch_idx * block_size) as u64;
                                // In real implementation, this would be async/non-blocking
                                let _ = dataset.read_at(
                                    SnapshotStream::Disk,
                                    prefetch_offset,
                                    block_size,
                                );
                            }
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks strided access patterns with prefetching.
///
/// Tests prefetch effectiveness when access pattern has stride > 1
/// (e.g., reading every Nth sample). This is common in certain data
/// augmentation strategies or when training on a subset of data.
fn bench_prefetch_strided_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("Prefetch/StridedAccess");

    let num_blocks = 1000;
    let block_size = 65536;
    let (_input, _output, snapshot) = common::create_dataset(num_blocks, block_size);
    let dataset = Arc::new(snapshot);

    // Test different stride lengths
    for stride in [1, 2, 4, 8, 16] {
        let num_accesses = num_blocks / stride;
        let total_bytes = (num_accesses * block_size) as u64;

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(BenchmarkId::new("stride", stride), &stride, |b, &s| {
            b.iter(|| {
                let mut i = 0;
                while i < num_blocks {
                    let offset = (i * block_size) as u64;
                    let data = dataset
                        .read_at(SnapshotStream::Disk, offset, block_size)
                        .unwrap();
                    black_box(data);
                    i += s;
                }
            });
        });
    }

    group.finish();
}

/// Benchmarks adaptive prefetch based on access pattern detection.
///
/// Simulates an adaptive prefetcher that detects sequential access and
/// adjusts window size accordingly. Compares against fixed window sizes.
fn bench_adaptive_prefetch(c: &mut Criterion) {
    let mut group = c.benchmark_group("Prefetch/Adaptive");

    let num_blocks = 1000;
    let block_size = 65536;
    let (_input, _output, snapshot) = common::create_dataset(num_blocks, block_size);
    let dataset = Arc::new(snapshot);
    let total_bytes = (num_blocks * block_size) as u64;

    group.throughput(Throughput::Bytes(total_bytes));

    // Fixed window (baseline)
    group.bench_function("FixedWindow8", |b| {
        b.iter(|| {
            for i in 0..num_blocks {
                let offset = (i * block_size) as u64;
                let data = dataset
                    .read_at(SnapshotStream::Disk, offset, block_size)
                    .unwrap();
                black_box(data);
            }
        });
    });

    // Adaptive window (increases on sequential, decreases on random)
    group.bench_function("Adaptive", |b| {
        b.iter(|| {
            let mut window_size = 4;
            let mut last_idx = 0usize;

            for i in 0..num_blocks {
                let offset = (i * block_size) as u64;
                let data = dataset
                    .read_at(SnapshotStream::Disk, offset, block_size)
                    .unwrap();
                black_box(data);

                // Adapt window based on sequentiality
                if i == last_idx + 1 {
                    // Sequential access detected - increase window
                    window_size = (window_size + 1).min(16);
                } else {
                    // Non-sequential - reduce window
                    window_size = (window_size - 1).max(1);
                }

                last_idx = i;
            }
        });
    });

    group.finish();
}

/// Benchmarks prefetch efficiency for different block sizes.
///
/// Tests how block size affects prefetch utility. Smaller blocks mean
/// more frequent prefetch opportunities but higher overhead. Larger
/// blocks amortize overhead but risk over-fetching unused data.
fn bench_prefetch_block_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("Prefetch/BlockSize");

    let total_data_size = 64 * 1024 * 1024; // 64MB total

    for block_size in [4096, 16384, 65536, 262144, 1048576] {
        let num_blocks = total_data_size / block_size;
        let (_input, _output, snapshot) = common::create_dataset(num_blocks, block_size);
        let dataset = Arc::new(snapshot);
        let window_size = 4; // Fixed window for comparison

        group.throughput(Throughput::Bytes(total_data_size as u64));
        group.bench_with_input(
            BenchmarkId::new("bytes", block_size),
            &block_size,
            |b, &bs| {
                b.iter(|| {
                    for i in 0..num_blocks {
                        let offset = (i * bs) as u64;
                        let data = dataset.read_at(SnapshotStream::Disk, offset, bs).unwrap();
                        black_box(data);

                        // Prefetch ahead
                        for j in 1..=window_size {
                            let prefetch_idx = i + j;
                            if prefetch_idx < num_blocks {
                                let prefetch_offset = (prefetch_idx * bs) as u64;
                                let _ = dataset.read_at(SnapshotStream::Disk, prefetch_offset, bs);
                            }
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks prefetch cache hit rate simulation.
///
/// Simulates cache behavior with different prefetch strategies to
/// estimate hit rates. This helps tune prefetch parameters for
/// optimal memory utilization.
fn bench_prefetch_hit_rate(c: &mut Criterion) {
    let mut group = c.benchmark_group("Prefetch/HitRate");

    let num_blocks = 500;
    let block_size = 65536;
    let (_input, _output, snapshot) = common::create_dataset(num_blocks, block_size);
    let dataset = Arc::new(snapshot);
    let cache_size = 50; // Cache can hold 50 blocks

    // Simulate LRU cache
    #[derive(Clone)]
    struct SimpleCache {
        entries: Vec<usize>,
        capacity: usize,
    }

    impl SimpleCache {
        fn new(capacity: usize) -> Self {
            Self {
                entries: Vec::new(),
                capacity,
            }
        }

        fn access(&mut self, block_idx: usize) -> bool {
            if let Some(pos) = self.entries.iter().position(|&x| x == block_idx) {
                // Hit - move to front (MRU)
                self.entries.remove(pos);
                self.entries.push(block_idx);
                true
            } else {
                // Miss - add to cache
                if self.entries.len() >= self.capacity {
                    self.entries.remove(0); // Evict LRU
                }
                self.entries.push(block_idx);
                false
            }
        }
    }

    // No prefetch baseline
    group.bench_function("NoPrefetch", |b| {
        b.iter(|| {
            let mut cache = SimpleCache::new(cache_size);
            let mut hits = 0;
            let mut misses = 0;

            for i in 0..num_blocks {
                if cache.access(i) {
                    hits += 1;
                } else {
                    misses += 1;
                    // Simulate read on miss
                    let offset = (i * block_size) as u64;
                    let _ = dataset.read_at(SnapshotStream::Disk, offset, block_size);
                }
            }
            black_box((hits, misses));
        });
    });

    // With prefetch
    for window_size in [2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("Prefetch", window_size),
            &window_size,
            |b, &window| {
                b.iter(|| {
                    let mut cache = SimpleCache::new(cache_size);
                    let mut hits = 0;
                    let mut misses = 0;

                    for i in 0..num_blocks {
                        if cache.access(i) {
                            hits += 1;
                        } else {
                            misses += 1;
                            let offset = (i * block_size) as u64;
                            let _ = dataset.read_at(SnapshotStream::Disk, offset, block_size);
                        }

                        // Prefetch future blocks
                        for j in 1..=window {
                            let prefetch_idx = i + j;
                            if prefetch_idx < num_blocks {
                                cache.access(prefetch_idx);
                            }
                        }
                    }
                    black_box((hits, misses));
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_prefetch_window_sizes,
    bench_prefetch_strided_access,
    bench_adaptive_prefetch,
    bench_prefetch_block_sizes,
    bench_prefetch_hit_rate
);
criterion_main!(benches);
