//! AI Data Loader Performance Benchmarks.
//!
//! Measures the performance characteristics of ML data loading patterns including
//! sequential iteration, random access with shuffling, and batch loading. These
//! benchmarks simulate real PyTorch/TensorFlow DataLoader workloads to validate
//! that Hexz can efficiently serve AI training pipelines.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use hexz_core::api::file::SnapshotStream;
use std::sync::Arc;

#[path = "common.rs"]
mod common;

/// Benchmarks sequential iteration through dataset samples.
///
/// Simulates epoch training where the DataLoader reads samples in order.
/// This pattern benefits from OS page cache and prefetching strategies.
fn bench_sequential_iteration(c: &mut Criterion) {
    let mut group = c.benchmark_group("DataLoader/Sequential");

    // Test with different dataset sizes (number of samples)
    for num_samples in [100, 1000, 10000] {
        let sample_size = 4096; // 4KB per sample (e.g., small images)
        let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
        let dataset = Arc::new(snapshot);
        let total_bytes = (num_samples * sample_size) as u64;

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(
            BenchmarkId::new("samples", num_samples),
            &num_samples,
            |b, _| {
                b.iter(|| {
                    let mut offset = 0u64;
                    for _ in 0..num_samples {
                        let data = dataset
                            .read_at(SnapshotStream::Disk, offset, sample_size)
                            .unwrap();
                        black_box(data);
                        offset += sample_size as u64;
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks random access pattern with index shuffling.
///
/// Simulates shuffled epoch training where samples are accessed in random
/// order. This is the most common pattern in ML training to prevent overfitting
/// but puts stress on the cache and can cause many cache misses.
fn bench_random_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("DataLoader/RandomAccess");

    for num_samples in [100, 1000, 5000] {
        let sample_size = 4096;
        let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
        let dataset = Arc::new(snapshot);
        let total_bytes = (num_samples * sample_size) as u64;

        // Pre-generate shuffled indices using Fisher-Yates
        let mut indices: Vec<usize> = (0..num_samples).collect();
        let mut state = 42u64; // Deterministic seed
        for i in (1..indices.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let j = (state as usize) % (i + 1);
            indices.swap(i, j);
        }

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(
            BenchmarkId::new("samples", num_samples),
            &num_samples,
            |b, _| {
                b.iter(|| {
                    for &idx in &indices {
                        let offset = (idx * sample_size) as u64;
                        let data = dataset
                            .read_at(SnapshotStream::Disk, offset, sample_size)
                            .unwrap();
                        black_box(data);
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks batch loading with various batch sizes.
///
/// Measures the cost of loading B consecutive samples (a batch) at once,
/// which is how ML frameworks typically load data. Tests if larger batch
/// sizes improve throughput through better amortization of I/O overhead.
fn bench_batch_loading(c: &mut Criterion) {
    let mut group = c.benchmark_group("DataLoader/Batching");

    let num_samples = 1000;
    let sample_size = 4096;
    let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
    let dataset = Arc::new(snapshot);

    // Test different batch sizes (common in ML: 1, 8, 16, 32, 64, 128)
    for batch_size in [1, 8, 16, 32, 64, 128] {
        let num_batches = num_samples / batch_size;
        let total_bytes = (num_samples * sample_size) as u64;

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(
            BenchmarkId::new("batch_size", batch_size),
            &batch_size,
            |b, &bs| {
                b.iter(|| {
                    for batch_idx in 0..num_batches {
                        // Load entire batch
                        for sample_in_batch in 0..bs {
                            let sample_idx = batch_idx * bs + sample_in_batch;
                            let offset = (sample_idx * sample_size) as u64;
                            let data = dataset
                                .read_at(SnapshotStream::Disk, offset, sample_size)
                                .unwrap();
                            black_box(data);
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks small vs. large sample sizes.
///
/// Tests how sample size affects throughput. Small samples (metadata, text tokens)
/// vs. large samples (high-res images, video frames) have different performance
/// characteristics due to decompression overhead and cache behavior.
fn bench_sample_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("DataLoader/SampleSize");

    let num_samples = 1000;

    // Test various sample sizes from 1KB (text) to 1MB (high-res images)
    for sample_size in [1024, 4096, 16384, 65536, 262144, 1048576] {
        let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
        let dataset = Arc::new(snapshot);
        let total_bytes = (num_samples * sample_size) as u64;

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(
            BenchmarkId::new("bytes", sample_size),
            &sample_size,
            |b, &size| {
                b.iter(|| {
                    let mut offset = 0u64;
                    for _ in 0..num_samples {
                        let data = dataset.read_at(SnapshotStream::Disk, offset, size).unwrap();
                        black_box(data);
                        offset += size as u64;
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks cache warmup vs. cold cache performance.
///
/// Measures the first-pass (cold cache) vs. second-pass (warm cache) iteration
/// to quantify the benefit of LRU caching for multi-epoch training.
fn bench_cache_warmup(c: &mut Criterion) {
    let mut group = c.benchmark_group("DataLoader/CacheWarmup");

    let num_samples = 1000;
    let sample_size = 4096;
    let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
    let dataset = Arc::new(snapshot);
    let total_bytes = (num_samples * sample_size) as u64;

    group.throughput(Throughput::Bytes(total_bytes));

    // Cold cache - first access
    group.bench_function("ColdCache", |b| {
        b.iter(|| {
            // Create fresh dataset for each iteration to simulate cold cache
            let (_i, _o, snap) = common::create_dataset(num_samples, sample_size);
            let mut offset = 0u64;
            for _ in 0..num_samples {
                let data = snap
                    .read_at(SnapshotStream::Disk, offset, sample_size)
                    .unwrap();
                black_box(data);
                offset += sample_size as u64;
            }
        });
    });

    // Warm cache - repeated access
    group.bench_function("WarmCache", |b| {
        // Warm up the cache once
        let mut offset = 0u64;
        for _ in 0..num_samples {
            let _ = dataset.read_at(SnapshotStream::Disk, offset, sample_size);
            offset += sample_size as u64;
        }

        b.iter(|| {
            let mut offset = 0u64;
            for _ in 0..num_samples {
                let data = dataset
                    .read_at(SnapshotStream::Disk, offset, sample_size)
                    .unwrap();
                black_box(data);
                offset += sample_size as u64;
            }
        });
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .warm_up_time(std::time::Duration::from_secs(2));
    targets = bench_sequential_iteration, bench_random_access, bench_batch_loading,
              bench_sample_sizes, bench_cache_warmup
}
criterion_main!(benches);
