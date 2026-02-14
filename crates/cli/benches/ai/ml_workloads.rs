//! ML Training Workload Benchmarks.
//!
//! Simulates realistic end-to-end ML training scenarios including:
//! - Multi-epoch training with different access patterns
//! - Training vs. validation data loading
//! - Data augmentation pipelines
//! - Checkpoint/resume behavior
//!
//! These benchmarks mirror real PyTorch/TensorFlow training loops.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use hexz_core::api::file::SnapshotStream;
use std::sync::Arc;

#[path = "common.rs"]
mod common;

/// Fisher-Yates shuffle for training.
fn shuffle_indices(count: usize, seed: u64) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..count).collect();
    let mut state = seed;
    for i in (1..indices.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state as usize) % (i + 1);
        indices.swap(i, j);
    }
    indices
}

/// Benchmarks multi-epoch training with shuffling between epochs.
///
/// Simulates a typical training loop:
/// 1. Shuffle dataset indices
/// 2. Iterate through all samples in shuffled order
/// 3. Repeat for N epochs
///
/// Tests cache effectiveness across epochs and shuffle overhead.
fn bench_multi_epoch_training(c: &mut Criterion) {
    let mut group = c.benchmark_group("MLWorkload/MultiEpoch");
    group.sample_size(10); // Reduce sample size for long-running benchmark

    let num_samples = 5000;
    let sample_size = 4096;
    let batch_size = 32;
    let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
    let dataset = Arc::new(snapshot);

    for num_epochs in [1, 3, 5, 10] {
        let total_bytes = (num_samples * sample_size * num_epochs) as u64;

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(
            BenchmarkId::new("epochs", num_epochs),
            &num_epochs,
            |b, &epochs| {
                b.iter(|| {
                    for epoch in 0..epochs {
                        // Shuffle with different seed per epoch
                        let indices = shuffle_indices(num_samples, 42 + epoch as u64);

                        // Iterate through all batches
                        for batch_start in (0..num_samples).step_by(batch_size) {
                            let batch_end = (batch_start + batch_size).min(num_samples);

                            for &idx in &indices[batch_start..batch_end] {
                                let offset = (idx * sample_size) as u64;
                                let data = dataset
                                    .read_at(SnapshotStream::Disk, offset, sample_size)
                                    .unwrap();
                                black_box(data);
                            }
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks train/validation split access patterns.
///
/// Simulates:
/// - Training phase: shuffled access to train set
/// - Validation phase: sequential access to validation set
///
/// This is the most common ML workflow pattern.
fn bench_train_val_split(c: &mut Criterion) {
    let mut group = c.benchmark_group("MLWorkload/TrainValSplit");

    let total_samples = 10000;
    let train_split = 0.8; // 80% train, 20% validation
    let train_samples = (total_samples as f64 * train_split) as usize;
    let val_samples = total_samples - train_samples;
    let sample_size = 4096;

    let (_input, _output, snapshot) = common::create_dataset(total_samples, sample_size);
    let dataset = Arc::new(snapshot);

    // Training phase (shuffled)
    group.bench_function("TrainingPhase", |b| {
        let indices = shuffle_indices(train_samples, 42);
        b.iter(|| {
            for &idx in &indices {
                let offset = (idx * sample_size) as u64;
                let data = dataset
                    .read_at(SnapshotStream::Disk, offset, sample_size)
                    .unwrap();
                black_box(data);
            }
        });
    });

    // Validation phase (sequential)
    group.bench_function("ValidationPhase", |b| {
        let val_start_offset = (train_samples * sample_size) as u64;
        b.iter(|| {
            let mut offset = val_start_offset;
            for _ in 0..val_samples {
                let data = dataset
                    .read_at(SnapshotStream::Disk, offset, sample_size)
                    .unwrap();
                black_box(data);
                offset += sample_size as u64;
            }
        });
    });

    // Full epoch (train + val)
    group.bench_function("FullEpoch", |b| {
        let train_indices = shuffle_indices(train_samples, 42);
        let val_start_offset = (train_samples * sample_size) as u64;

        b.iter(|| {
            // Training
            for &idx in &train_indices {
                let offset = (idx * sample_size) as u64;
                let data = dataset
                    .read_at(SnapshotStream::Disk, offset, sample_size)
                    .unwrap();
                black_box(data);
            }

            // Validation
            let mut offset = val_start_offset;
            for _ in 0..val_samples {
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

/// Benchmarks different batch size effects on throughput.
///
/// Tests how batch size affects data loading throughput. Larger batches
/// amortize overhead but may reduce randomness and cache effectiveness.
fn bench_batch_size_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("MLWorkload/BatchSize");

    let num_samples = 10000;
    let sample_size = 4096;
    let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
    let dataset = Arc::new(snapshot);
    let indices = shuffle_indices(num_samples, 42);

    for batch_size in [1, 8, 16, 32, 64, 128, 256, 512] {
        let total_bytes = (num_samples * sample_size) as u64;

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(
            BenchmarkId::new("size", batch_size),
            &batch_size,
            |b, &bs| {
                b.iter(|| {
                    for batch_start in (0..num_samples).step_by(bs) {
                        let batch_end = (batch_start + bs).min(num_samples);

                        for &idx in &indices[batch_start..batch_end] {
                            let offset = (idx * sample_size) as u64;
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

/// Benchmarks checkpoint/resume overhead.
///
/// Simulates resuming training from a checkpoint by seeking to a specific
/// position in the dataset. Tests the cost of random access after a long
/// sequential read.
fn bench_checkpoint_resume(c: &mut Criterion) {
    let mut group = c.benchmark_group("MLWorkload/CheckpointResume");

    let num_samples = 10000;
    let sample_size = 4096;
    let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
    let dataset = Arc::new(snapshot);
    let indices = shuffle_indices(num_samples, 42);

    // Resume from different points
    for resume_pct in [0, 25, 50, 75] {
        let resume_idx = (num_samples * resume_pct) / 100;
        let remaining_samples = num_samples - resume_idx;
        let total_bytes = (remaining_samples * sample_size) as u64;

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(
            BenchmarkId::new("resume_pct", resume_pct),
            &resume_pct,
            |b, _| {
                b.iter(|| {
                    // Resume from checkpoint position
                    for &idx in &indices[resume_idx..] {
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

/// Benchmarks data augmentation pipeline overhead.
///
/// Simulates applying transformations to loaded data (resize, crop, normalize).
/// Tests the proportion of time spent in I/O vs. compute.
fn bench_augmentation_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("MLWorkload/Augmentation");

    let num_samples = 1000;
    let h = 224;
    let w = 224;
    let c = 3;
    let sample_size = h * w * c;
    let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
    let dataset = Arc::new(snapshot);

    // No augmentation (baseline)
    group.bench_function("NoAugmentation", |b| {
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

    // Light augmentation (normalize only)
    group.bench_function("Normalize", |b| {
        b.iter(|| {
            let mut offset = 0u64;
            for _ in 0..num_samples {
                let data = dataset
                    .read_at(SnapshotStream::Disk, offset, sample_size)
                    .unwrap();

                // Normalize to [0, 1]
                let normalized: Vec<f32> = data.iter().map(|&x| x as f32 / 255.0).collect();

                black_box(normalized);
                offset += sample_size as u64;
            }
        });
    });

    // Heavy augmentation (transpose + normalize + scale)
    group.bench_function("HeavyAugmentation", |b| {
        b.iter(|| {
            let mut offset = 0u64;
            for _ in 0..num_samples {
                let data = dataset
                    .read_at(SnapshotStream::Disk, offset, sample_size)
                    .unwrap();

                // HWC to CHW transpose
                let mut transposed = vec![0u8; sample_size];
                for y in 0..h {
                    for x in 0..w {
                        for ch in 0..c {
                            let src_idx = (y * w + x) * c + ch;
                            let dst_idx = ch * h * w + y * w + x;
                            transposed[dst_idx] = data[src_idx];
                        }
                    }
                }

                // Normalize and scale
                let normalized: Vec<f32> = transposed
                    .iter()
                    .map(|&x| (x as f32 / 255.0 - 0.5) * 2.0)
                    .collect();

                black_box(normalized);
                offset += sample_size as u64;
            }
        });
    });

    group.finish();
}

/// Benchmarks drop_last behavior (common in PyTorch).
///
/// Tests the difference between processing all samples vs. dropping the
/// last incomplete batch. This affects the number of samples processed
/// and can impact cache behavior.
fn bench_drop_last_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("MLWorkload/DropLast");

    let num_samples = 10007; // Not evenly divisible by common batch sizes
    let sample_size = 4096;
    let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
    let dataset = Arc::new(snapshot);
    let batch_size = 32;

    // Keep all samples (process incomplete batch)
    group.bench_function("KeepAll", |b| {
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

    // Drop last incomplete batch
    group.bench_function("DropLast", |b| {
        let num_full_batches = num_samples / batch_size;
        let samples_to_read = num_full_batches * batch_size;

        b.iter(|| {
            let mut offset = 0u64;
            for _ in 0..samples_to_read {
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

/// Benchmarks dataset subset sampling.
///
/// Tests loading a random subset of the dataset (e.g., for quick validation
/// or hyperparameter tuning). Measures cost of sparse random access.
fn bench_subset_sampling(c: &mut Criterion) {
    let mut group = c.benchmark_group("MLWorkload/SubsetSampling");

    let num_samples = 10000;
    let sample_size = 4096;
    let (_input, _output, snapshot) = common::create_dataset(num_samples, sample_size);
    let dataset = Arc::new(snapshot);

    // Sample different percentages
    for subset_pct in [10, 25, 50, 75, 100] {
        let subset_size = (num_samples * subset_pct) / 100;
        let total_bytes = (subset_size * sample_size) as u64;

        // Generate random subset indices
        let subset_indices = shuffle_indices(num_samples, 42)
            .into_iter()
            .take(subset_size)
            .collect::<Vec<_>>();

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(
            BenchmarkId::new("percent", subset_pct),
            &subset_pct,
            |b, _| {
                b.iter(|| {
                    for &idx in &subset_indices {
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

criterion_group!(
    benches,
    bench_multi_epoch_training,
    bench_train_val_split,
    bench_batch_size_scaling,
    bench_checkpoint_resume,
    bench_augmentation_pipeline,
    bench_drop_last_batch,
    bench_subset_sampling
);
criterion_main!(benches);
