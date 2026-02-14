//! Multi-Worker Data Loading Benchmarks.
//!
//! Simulates PyTorch/TensorFlow DataLoader behavior with multiple worker threads
//! loading data in parallel. This is essential for keeping GPUs fed during training
//! by overlapping data loading with model computation.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use hexz_core::api::file::SnapshotStream;
use std::sync::{Arc, Barrier};
use std::thread;

#[path = "common.rs"]
mod common;

/// Benchmarks throughput scaling with multiple worker threads.
///
/// Tests how well Hexz's read path scales when multiple threads are
/// reading different parts of the dataset simultaneously. Ideal for
/// multi-GPU training scenarios.
fn bench_worker_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("MultiWorker/Scaling");

    let num_samples = 10000;
    let sample_size = 4096;
    let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
    let dataset = Arc::new(snapshot);
    let total_bytes = (num_samples * sample_size) as u64;

    // Test with 1, 2, 4, 8, 16 workers (common DataLoader configurations)
    for num_workers in [1, 2, 4, 8, 16] {
        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(
            BenchmarkId::new("workers", num_workers),
            &num_workers,
            |b, &n_workers| {
                b.iter(|| {
                    let barrier = Arc::new(Barrier::new(n_workers));
                    let samples_per_worker = num_samples / n_workers;

                    let handles: Vec<_> = (0..n_workers)
                        .map(|worker_id| {
                            let dataset = dataset.clone();
                            let barrier = barrier.clone();
                            thread::spawn(move || {
                                // Synchronize start
                                barrier.wait();

                                // Each worker reads its partition
                                let start_idx = worker_id * samples_per_worker;
                                let end_idx = start_idx + samples_per_worker;

                                for idx in start_idx..end_idx {
                                    let offset = (idx * sample_size) as u64;
                                    let data = dataset
                                        .read_at(SnapshotStream::Disk, offset, sample_size)
                                        .unwrap();
                                    black_box(data);
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks contention when workers access overlapping data.
///
/// Tests performance when multiple workers read the same blocks (e.g.,
/// due to data augmentation or overlapping batches). Measures cache
/// contention and locking overhead.
fn bench_worker_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("MultiWorker/Contention");

    let num_samples = 1000;
    let sample_size = 4096;
    let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
    let dataset = Arc::new(snapshot);
    let num_workers = 8;

    // No overlap - each worker reads different data
    group.bench_function("NoOverlap", |b| {
        b.iter(|| {
            let barrier = Arc::new(Barrier::new(num_workers));
            let samples_per_worker = num_samples / num_workers;

            let handles: Vec<_> = (0..num_workers)
                .map(|worker_id| {
                    let dataset = dataset.clone();
                    let barrier = barrier.clone();
                    thread::spawn(move || {
                        barrier.wait();
                        let start = worker_id * samples_per_worker;
                        let end = start + samples_per_worker;

                        for idx in start..end {
                            let offset = (idx * sample_size) as u64;
                            let data = dataset
                                .read_at(SnapshotStream::Disk, offset, sample_size)
                                .unwrap();
                            black_box(data);
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }
        });
    });

    // Full overlap - all workers read same data
    group.bench_function("FullOverlap", |b| {
        b.iter(|| {
            let barrier = Arc::new(Barrier::new(num_workers));
            let samples_to_read = num_samples / num_workers;

            let handles: Vec<_> = (0..num_workers)
                .map(|_| {
                    let dataset = dataset.clone();
                    let barrier = barrier.clone();
                    thread::spawn(move || {
                        barrier.wait();

                        // All workers read the same samples
                        for idx in 0..samples_to_read {
                            let offset = (idx * sample_size) as u64;
                            let data = dataset
                                .read_at(SnapshotStream::Disk, offset, sample_size)
                                .unwrap();
                            black_box(data);
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }
        });
    });

    // Partial overlap - 50% data sharing
    group.bench_function("PartialOverlap", |b| {
        b.iter(|| {
            let barrier = Arc::new(Barrier::new(num_workers));
            let samples_per_worker = num_samples / num_workers;

            let handles: Vec<_> = (0..num_workers)
                .map(|worker_id| {
                    let dataset = dataset.clone();
                    let barrier = barrier.clone();
                    thread::spawn(move || {
                        barrier.wait();

                        // Each worker reads half unique, half shared data
                        let unique_start = worker_id * samples_per_worker;
                        let unique_count = samples_per_worker / 2;
                        let shared_count = samples_per_worker - unique_count;

                        // Unique data
                        for i in 0..unique_count {
                            let idx = unique_start + i;
                            let offset = (idx * sample_size) as u64;
                            let data = dataset
                                .read_at(SnapshotStream::Disk, offset, sample_size)
                                .unwrap();
                            black_box(data);
                        }

                        // Shared data (first N samples)
                        for i in 0..shared_count {
                            let offset = (i * sample_size) as u64;
                            let data = dataset
                                .read_at(SnapshotStream::Disk, offset, sample_size)
                                .unwrap();
                            black_box(data);
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }
        });
    });

    group.finish();
}

/// Benchmarks round-robin worker assignment pattern.
///
/// Simulates how PyTorch DataLoader distributes samples to workers in
/// round-robin fashion. This is the most common work distribution strategy.
fn bench_round_robin_distribution(c: &mut Criterion) {
    let mut group = c.benchmark_group("MultiWorker/RoundRobin");

    let num_samples = 8000;
    let sample_size = 4096;
    let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
    let dataset = Arc::new(snapshot);

    for num_workers in [2, 4, 8] {
        let total_bytes = (num_samples * sample_size) as u64;

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(
            BenchmarkId::new("workers", num_workers),
            &num_workers,
            |b, &n_workers| {
                b.iter(|| {
                    let barrier = Arc::new(Barrier::new(n_workers));

                    let handles: Vec<_> = (0..n_workers)
                        .map(|worker_id| {
                            let dataset = dataset.clone();
                            let barrier = barrier.clone();
                            thread::spawn(move || {
                                barrier.wait();

                                // Round-robin: worker i reads samples i, i+n, i+2n, ...
                                let mut idx = worker_id;
                                while idx < num_samples {
                                    let offset = (idx * sample_size) as u64;
                                    let data = dataset
                                        .read_at(SnapshotStream::Disk, offset, sample_size)
                                        .unwrap();
                                    black_box(data);
                                    idx += n_workers;
                                }
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks worker load imbalance handling.
///
/// Tests scenarios where workers finish at different times due to
/// unequal work distribution or varying data complexity. Measures
/// how well the system handles stragglers.
fn bench_load_imbalance(c: &mut Criterion) {
    let mut group = c.benchmark_group("MultiWorker/LoadBalance");

    let num_samples = 8000;
    let sample_size = 4096;
    let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
    let dataset = Arc::new(snapshot);
    let num_workers = 8;

    // Balanced workload
    group.bench_function("Balanced", |b| {
        b.iter(|| {
            let barrier = Arc::new(Barrier::new(num_workers));
            let samples_per_worker = num_samples / num_workers;

            let handles: Vec<_> = (0..num_workers)
                .map(|worker_id| {
                    let dataset = dataset.clone();
                    let barrier = barrier.clone();
                    thread::spawn(move || {
                        barrier.wait();

                        let start = worker_id * samples_per_worker;
                        let end = start + samples_per_worker;

                        for idx in start..end {
                            let offset = (idx * sample_size) as u64;
                            let data = dataset
                                .read_at(SnapshotStream::Disk, offset, sample_size)
                                .unwrap();
                            black_box(data);
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }
        });
    });

    // Imbalanced - one worker does 2x work
    group.bench_function("OneStraggler", |b| {
        b.iter(|| {
            let barrier = Arc::new(Barrier::new(num_workers));
            let base_samples = num_samples / (num_workers + 1);

            let handles: Vec<_> = (0..num_workers)
                .map(|worker_id| {
                    let dataset = dataset.clone();
                    let barrier = barrier.clone();
                    thread::spawn(move || {
                        barrier.wait();

                        // Worker 0 does 2x work
                        let samples = if worker_id == 0 {
                            base_samples * 2
                        } else {
                            base_samples
                        };

                        let start = if worker_id == 0 {
                            0
                        } else {
                            base_samples * 2 + (worker_id - 1) * base_samples
                        };

                        for i in 0..samples {
                            let idx = start + i;
                            if idx < num_samples {
                                let offset = (idx * sample_size) as u64;
                                let data = dataset
                                    .read_at(SnapshotStream::Disk, offset, sample_size)
                                    .unwrap();
                                black_box(data);
                            }
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }
        });
    });

    group.finish();
}

/// Benchmarks worker startup and teardown overhead.
///
/// Measures the cost of spawning and joining worker threads, which
/// happens at the start of each epoch in typical training loops.
fn bench_worker_lifecycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("MultiWorker/Lifecycle");

    let num_samples = 1000;
    let sample_size = 4096;
    let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
    let dataset = Arc::new(snapshot);

    for num_workers in [2, 4, 8, 16] {
        group.bench_with_input(
            BenchmarkId::new("workers", num_workers),
            &num_workers,
            |b, &n_workers| {
                b.iter(|| {
                    let samples_per_worker = num_samples / n_workers;

                    // Spawn workers
                    let handles: Vec<_> = (0..n_workers)
                        .map(|worker_id| {
                            let dataset = dataset.clone();
                            thread::spawn(move || {
                                let start = worker_id * samples_per_worker;
                                let end = start + samples_per_worker;

                                for idx in start..end {
                                    let offset = (idx * sample_size) as u64;
                                    let data = dataset
                                        .read_at(SnapshotStream::Disk, offset, sample_size)
                                        .unwrap();
                                    black_box(data);
                                }
                            })
                        })
                        .collect();

                    // Join workers
                    for handle in handles {
                        handle.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_worker_scaling,
    bench_worker_contention,
    bench_round_robin_distribution,
    bench_load_imbalance,
    bench_worker_lifecycle
);
criterion_main!(benches);
