//! Tensor Operation Benchmarks.
//!
//! Measures the performance of zero-copy tensor operations and buffer protocol
//! integration used when transferring data from Hexz to Python/NumPy/PyTorch.
//! These benchmarks validate that data transfer overhead is minimal.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use hexz_core::api::file::SnapshotStream;
use std::sync::Arc;

#[path = "common.rs"]
mod common;

/// Benchmarks reading tensors of different sizes.
///
/// Tests tensor loading performance across common ML tensor sizes:
/// - Small: Embeddings, labels (few KB)
/// - Medium: Small images, audio chunks (tens to hundreds of KB)
/// - Large: High-res images, video frames (MB range)
fn bench_tensor_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("TensorOps/SizeScaling");

    let num_tensors = 100;

    // Common tensor sizes in bytes
    let tensor_configs = [
        (768, "Embedding768"),          // BERT base embedding
        (3072, "Embedding3K"),          // GPT-like embedding
        (28 * 28, "MNIST"),             // MNIST image
        (32 * 32 * 3, "CIFAR10"),       // CIFAR-10 image
        (224 * 224 * 3, "ImageNet224"), // ImageNet image
        (512 * 512 * 3, "HighRes512"),  // High-res image
        (1024 * 1024 * 3, "HighRes1K"), // Very high-res image
    ];

    for (tensor_size, label) in tensor_configs {
        let (_input, _output, snapshot) = common::create_dataset(num_tensors, tensor_size);
        let dataset = Arc::new(snapshot);
        let total_bytes = (num_tensors * tensor_size) as u64;

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(BenchmarkId::new("type", label), &tensor_size, |b, &size| {
            b.iter(|| {
                let mut offset = 0u64;
                for _ in 0..num_tensors {
                    let data = dataset.read_at(SnapshotStream::Disk, offset, size).unwrap();
                    black_box(data);
                    offset += size as u64;
                }
            });
        });
    }

    group.finish();
}

/// Benchmarks zero-copy vs. copy tensor operations.
///
/// Compares the cost of returning a direct buffer reference vs. copying
/// data into a new allocation. Zero-copy is critical for performance when
/// interfacing with Python/NumPy.
fn bench_zero_copy_vs_copy(c: &mut Criterion) {
    let mut group = c.benchmark_group("TensorOps/ZeroCopy");

    let num_tensors = 100;
    let tensor_size = 224 * 224 * 3; // ImageNet size
    let (_input, _output, snapshot) = common::create_dataset(num_tensors, tensor_size);
    let dataset = Arc::new(snapshot);
    let total_bytes = (num_tensors * tensor_size) as u64;

    group.throughput(Throughput::Bytes(total_bytes));

    // Zero-copy path (return Vec directly)
    group.bench_function("ZeroCopy", |b| {
        b.iter(|| {
            let mut offset = 0u64;
            for _ in 0..num_tensors {
                let data = dataset
                    .read_at(SnapshotStream::Disk, offset, tensor_size)
                    .unwrap();
                black_box(data);
                offset += tensor_size as u64;
            }
        });
    });

    // Copy path (clone the data)
    group.bench_function("WithCopy", |b| {
        b.iter(|| {
            let mut offset = 0u64;
            for _ in 0..num_tensors {
                let data = dataset
                    .read_at(SnapshotStream::Disk, offset, tensor_size)
                    .unwrap();
                let copied = data.clone();
                black_box(copied);
                offset += tensor_size as u64;
            }
        });
    });

    group.finish();
}

/// Benchmarks batched tensor loading.
///
/// Tests loading multiple tensors at once (a batch) vs. loading them
/// individually. Batching amortizes function call overhead and enables
/// better prefetching.
fn bench_batch_tensor_loading(c: &mut Criterion) {
    let mut group = c.benchmark_group("TensorOps/Batching");

    let num_tensors = 128;
    let tensor_size = 224 * 224 * 3;
    let (_input, _output, snapshot) = common::create_dataset(num_tensors, tensor_size);
    let dataset = Arc::new(snapshot);

    for batch_size in [1, 4, 8, 16, 32, 64] {
        let num_batches = num_tensors / batch_size;
        let total_bytes = (num_tensors * tensor_size) as u64;

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(
            BenchmarkId::new("batch", batch_size),
            &batch_size,
            |b, &bs| {
                b.iter(|| {
                    for batch_idx in 0..num_batches {
                        let mut batch_data = Vec::with_capacity(bs);

                        for i in 0..bs {
                            let tensor_idx = batch_idx * bs + i;
                            let offset = (tensor_idx * tensor_size) as u64;
                            let data = dataset
                                .read_at(SnapshotStream::Disk, offset, tensor_size)
                                .unwrap();
                            batch_data.push(data);
                        }

                        black_box(batch_data);
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks tensor reshape/transpose overhead.
///
/// Simulates common preprocessing operations that happen after loading
/// tensors but before feeding to models. Tests the cost of memory
/// reorganization operations.
fn bench_tensor_preprocessing(c: &mut Criterion) {
    let mut group = c.benchmark_group("TensorOps/Preprocessing");

    let num_tensors = 100;
    let h = 224;
    let w = 224;
    let c = 3;
    let tensor_size = h * w * c;
    let (_input, _output, snapshot) = common::create_dataset(num_tensors, tensor_size);
    let dataset = Arc::new(snapshot);

    // Baseline: just load
    group.bench_function("LoadOnly", |b| {
        b.iter(|| {
            let mut offset = 0u64;
            for _ in 0..num_tensors {
                let data = dataset
                    .read_at(SnapshotStream::Disk, offset, tensor_size)
                    .unwrap();
                black_box(data);
                offset += tensor_size as u64;
            }
        });
    });

    // Load + reshape (HWC to CHW - common in PyTorch)
    group.bench_function("LoadAndReshape", |b| {
        b.iter(|| {
            let mut offset = 0u64;
            for _ in 0..num_tensors {
                let data = dataset
                    .read_at(SnapshotStream::Disk, offset, tensor_size)
                    .unwrap();

                // Simulate HWC to CHW transpose
                let mut transposed = vec![0u8; tensor_size];
                for y in 0..h {
                    for x in 0..w {
                        for ch in 0..c {
                            let src_idx = (y * w + x) * c + ch;
                            let dst_idx = ch * h * w + y * w + x;
                            transposed[dst_idx] = data[src_idx];
                        }
                    }
                }

                black_box(transposed);
                offset += tensor_size as u64;
            }
        });
    });

    // Load + normalize (scale to [0, 1])
    group.bench_function("LoadAndNormalize", |b| {
        b.iter(|| {
            let mut offset = 0u64;
            for _ in 0..num_tensors {
                let data = dataset
                    .read_at(SnapshotStream::Disk, offset, tensor_size)
                    .unwrap();

                // Simulate normalization (u8 -> f32 / 255.0)
                let normalized: Vec<f32> = data.iter().map(|&x| x as f32 / 255.0).collect();

                black_box(normalized);
                offset += tensor_size as u64;
            }
        });
    });

    group.finish();
}

/// Benchmarks tensor alignment and padding overhead.
///
/// Tests the cost of ensuring tensors are properly aligned for SIMD
/// operations or GPU transfer. Alignment is often required for optimal
/// hardware utilization.
fn bench_tensor_alignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("TensorOps/Alignment");

    let num_tensors = 100;

    // Test with tensors that are naturally aligned vs. misaligned
    for (tensor_size, label) in [
        (32768, "Aligned64"),    // 32KB - aligned to 64 bytes
        (32769, "Misaligned1"),  // Off by 1 byte
        (32800, "Misaligned32"), // Off by 32 bytes
    ] {
        let (_input, _output, snapshot) = common::create_dataset(num_tensors, tensor_size);
        let dataset = Arc::new(snapshot);
        let total_bytes = (num_tensors * tensor_size) as u64;

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(BenchmarkId::new("size", label), &tensor_size, |b, &size| {
            b.iter(|| {
                let mut offset = 0u64;
                for _ in 0..num_tensors {
                    let data = dataset.read_at(SnapshotStream::Disk, offset, size).unwrap();

                    // Simulate ensuring 64-byte alignment
                    let aligned = if data.as_ptr() as usize % 64 == 0 {
                        data
                    } else {
                        // Need to copy to aligned buffer
                        let mut aligned_buf = vec![0u8; size];
                        aligned_buf.copy_from_slice(&data);
                        aligned_buf
                    };

                    black_box(aligned);
                    offset += size as u64;
                }
            });
        });
    }

    group.finish();
}

/// Benchmarks tensor concatenation operations.
///
/// Tests the cost of concatenating multiple tensors into a single
/// contiguous buffer, which is needed when batching variable-length
/// sequences or stacking images.
fn bench_tensor_concat(c: &mut Criterion) {
    let mut group = c.benchmark_group("TensorOps/Concatenation");

    let tensor_size = 4096;
    let (_input, _output, snapshot) = common::create_dataset(100, tensor_size);
    let dataset = Arc::new(snapshot);

    // Test concatenating different numbers of tensors
    for num_concat in [2, 4, 8, 16, 32] {
        let total_bytes = (num_concat * tensor_size) as u64;

        group.throughput(Throughput::Bytes(total_bytes));
        group.bench_with_input(
            BenchmarkId::new("tensors", num_concat),
            &num_concat,
            |b, &n| {
                b.iter(|| {
                    // Load individual tensors
                    let mut tensors = Vec::new();
                    for i in 0..n {
                        let offset = (i * tensor_size) as u64;
                        let data = dataset
                            .read_at(SnapshotStream::Disk, offset, tensor_size)
                            .unwrap();
                        tensors.push(data);
                    }

                    // Concatenate into single buffer
                    let mut concatenated = Vec::with_capacity(n * tensor_size);
                    for tensor in tensors {
                        concatenated.extend_from_slice(&tensor);
                    }

                    black_box(concatenated);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_tensor_sizes,
    bench_zero_copy_vs_copy,
    bench_batch_tensor_loading,
    bench_tensor_preprocessing,
    bench_tensor_alignment,
    bench_tensor_concat
);
criterion_main!(benches);
