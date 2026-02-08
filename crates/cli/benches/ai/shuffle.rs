//! Index Shuffling Performance Benchmarks.
//!
//! Measures the performance of Fisher-Yates shuffling algorithm at various scales
//! relevant to ML training. Shuffling is performed once per epoch in typical training
//! pipelines, and its cost should be negligible compared to data loading itself.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};

/// Fisher-Yates shuffle implementation using xorshift64 PRNG.
///
/// This is the same deterministic shuffle used by the Strata data loader.
/// We benchmark it separately to understand its scaling characteristics.
fn shuffled_indices(count: usize, seed: u64) -> Vec<usize> {
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

/// Benchmarks shuffle performance across dataset sizes from 1K to 10M samples.
///
/// Typical ML datasets range from thousands to millions of samples. This benchmark
/// validates that shuffle overhead remains acceptable even for very large datasets.
fn bench_shuffle_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("Shuffle/Scaling");

    // Dataset sizes from small (1K) to very large (10M)
    for count in [
        1_000,      // Small dataset (quick iteration)
        10_000,     // Medium dataset (MNIST, CIFAR)
        100_000,    // Large dataset (ImageNet subset)
        1_000_000,  // Very large dataset (full ImageNet)
        10_000_000, // Massive dataset (web-scale)
    ] {
        group.throughput(Throughput::Elements(count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), &count, |b, &n| {
            b.iter(|| {
                let indices = shuffled_indices(n, 42);
                black_box(indices);
            });
        });
    }

    group.finish();
}

/// Benchmarks memory allocation cost vs. in-place shuffling.
///
/// Measures the overhead of allocating the index vector vs. performing
/// the shuffle operations themselves. This helps understand whether allocation
/// or computation dominates the shuffle cost.
fn bench_shuffle_components(c: &mut Criterion) {
    let mut group = c.benchmark_group("Shuffle/Components");

    let count = 1_000_000;

    // Just allocation
    group.bench_function("Allocation", |b| {
        b.iter(|| {
            let indices: Vec<usize> = (0..count).collect();
            black_box(indices);
        });
    });

    // Just shuffling (pre-allocated)
    group.bench_function("Permutation", |b| {
        let mut indices: Vec<usize> = (0..count).collect();
        b.iter(|| {
            let mut state = 42u64;
            for i in (1..indices.len()).rev() {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let j = (state as usize) % (i + 1);
                indices.swap(i, j);
            }
            black_box(&indices);
        });
    });

    // Full shuffle (allocation + permutation)
    group.bench_function("Complete", |b| {
        b.iter(|| {
            let indices = shuffled_indices(count, 42);
            black_box(indices);
        });
    });

    group.finish();
}

/// Benchmarks determinism verification cost.
///
/// In some training scenarios, you need to verify that the shuffle is
/// deterministic (same seed produces same ordering). This measures the
/// cost of generating and comparing shuffled sequences.
fn bench_shuffle_determinism(c: &mut Criterion) {
    let mut group = c.benchmark_group("Shuffle/Determinism");

    let count = 100_000;

    group.bench_function("GenerateTwice", |b| {
        b.iter(|| {
            let a = shuffled_indices(count, 42);
            let b = shuffled_indices(count, 42);
            black_box((a, b));
        });
    });

    group.bench_function("GenerateAndCompare", |b| {
        b.iter(|| {
            let a = shuffled_indices(count, 42);
            let b = shuffled_indices(count, 42);
            let equal = a == b;
            black_box(equal);
        });
    });

    group.finish();
}

/// Benchmarks shuffle with different PRNG quality levels.
///
/// Compares the xorshift64 PRNG used in Strata against a simpler modulo-based
/// approach and a more complex xorshift128 implementation to understand the
/// performance-quality tradeoff.
fn bench_shuffle_prng_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("Shuffle/PRNGComparison");

    let count = 1_000_000;

    // Xorshift64 (current implementation)
    group.bench_function("Xorshift64", |b| {
        b.iter(|| {
            let mut indices: Vec<usize> = (0..count).collect();
            let mut state = 42u64;
            for i in (1..indices.len()).rev() {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let j = (state as usize) % (i + 1);
                indices.swap(i, j);
            }
            black_box(indices);
        });
    });

    // Simple modulo (fast but poor quality)
    group.bench_function("SimpleModulo", |b| {
        b.iter(|| {
            let mut indices: Vec<usize> = (0..count).collect();
            let mut state = 42u64;
            for i in (1..indices.len()).rev() {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                let j = (state as usize) % (i + 1);
                indices.swap(i, j);
            }
            black_box(indices);
        });
    });

    // Xorshift128 (higher quality but slower)
    group.bench_function("Xorshift128", |b| {
        b.iter(|| {
            let mut indices: Vec<usize> = (0..count).collect();
            let mut state = [42u64, 1, 2, 3];
            for i in (1..indices.len()).rev() {
                let t = state[3];
                let s = state[0];
                state[3] = state[2];
                state[2] = state[1];
                state[1] = s;

                let t = t ^ (t << 11);
                let t = t ^ (t >> 8);
                state[0] = t ^ s ^ (s >> 19);

                let j = (state[0] as usize) % (i + 1);
                indices.swap(i, j);
            }
            black_box(indices);
        });
    });

    group.finish();
}

/// Benchmarks cache locality during shuffle access.
///
/// Measures the cost of actually using shuffled indices to access data
/// vs. sequential access, quantifying the cache miss penalty.
fn bench_shuffle_access_pattern(c: &mut Criterion) {
    let mut group = c.benchmark_group("Shuffle/AccessPattern");

    let count = 10_000;
    let sample_size = 4096; // 4KB per sample

    // Create fake data array
    let data: Vec<u8> = vec![0u8; count * sample_size];

    // Sequential access
    group.bench_function("Sequential", |b| {
        b.iter(|| {
            for i in 0..count {
                let offset = i * sample_size;
                let slice = &data[offset..offset + sample_size];
                black_box(slice);
            }
        });
    });

    // Shuffled access
    group.bench_function("Shuffled", |b| {
        let indices = shuffled_indices(count, 42);
        b.iter(|| {
            for &i in &indices {
                let offset = i * sample_size;
                let slice = &data[offset..offset + sample_size];
                black_box(slice);
            }
        });
    });

    // Reverse access (anti-sequential)
    group.bench_function("Reverse", |b| {
        b.iter(|| {
            for i in (0..count).rev() {
                let offset = i * sample_size;
                let slice = &data[offset..offset + sample_size];
                black_box(slice);
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_shuffle_scaling,
    bench_shuffle_components,
    bench_shuffle_determinism,
    bench_shuffle_prng_comparison,
    bench_shuffle_access_pattern
);
criterion_main!(benches);
