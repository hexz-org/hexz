//! FastCDC Chunking Micro-Benchmark.
//!
//! This module measures the raw throughput of the FastCDC (Content-Defined Chunking)
//! algorithm without compression or storage overhead. It tests CDC performance across
//! different data patterns to understand chunking throughput and validate the claimed
//! ~500 MB/s performance.
//!
//! The benchmark isolates chunking overhead by:
//! - Testing on different data patterns (random, compressible, zeros, repeated)
//! - Measuring only chunk boundary detection (no compression or I/O)
//! - Comparing against fixed-size chunking baseline
//! - Analyzing chunk size distribution characteristics

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use hexz_core::algo::dedup::cdc::StreamChunker;
use hexz_core::algo::dedup::dcam::DedupeParams;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::io::Cursor;

/// Generates random data for chunking tests.
fn generate_random(size: usize) -> Vec<u8> {
    let mut rng = StdRng::seed_from_u64(42);
    let mut data = Vec::with_capacity(size);
    for _ in 0..size {
        data.push(rng.r#gen::<u8>());
    }
    data
}

/// Generates compressible pattern data (repeating pattern with variation).
fn generate_pattern(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size);
    for i in 0..size {
        data.push(((i / 64) % 256) as u8);
    }
    data
}

/// Generates repeated blocks (highly redundant data).
fn generate_repeated(size: usize) -> Vec<u8> {
    let block = b"This is a repeated block of text that will appear many times in the dataset to test CDC chunking performance on redundant data. ";
    let mut data = Vec::with_capacity(size);
    while data.len() < size {
        data.extend_from_slice(block);
    }
    data.truncate(size);
    data
}

/// Benchmarks FastCDC chunking throughput across different data patterns.
///
/// This function measures:
/// 1. Chunking throughput (MB/s) for various data types
/// 2. Chunk count and average chunk size
/// 3. Performance overhead vs fixed-size chunking
fn bench_fastcdc(c: &mut Criterion) {
    let mut group = c.benchmark_group("FastCDC");

    let data_size = 10_000_000; // 10 MB test data
    group.throughput(Throughput::Bytes(data_size as u64));

    // Default parameters: 16KB average, 2KB min, 64KB max
    let params = DedupeParams {
        f: 14,    // 2^14 = 16KB average
        m: 2048,  // 2KB minimum
        z: 65536, // 64KB maximum
        w: 48,    // 48-byte window
        v: 16,    // 16-byte metadata overhead per chunk
    };

    // Test 1: Random data (worst case for compression, baseline for CDC)
    let random_data = generate_random(data_size);
    group.bench_function("Random", |b| {
        b.iter(|| {
            let chunker = StreamChunker::new(Cursor::new(&random_data), params);
            let mut count = 0;
            for chunk_result in chunker {
                let _chunk = black_box(chunk_result.unwrap());
                count += 1;
            }
            count
        });
    });

    // Test 2: Compressible pattern data
    let pattern_data = generate_pattern(data_size);
    group.bench_function("Compressible", |b| {
        b.iter(|| {
            let chunker = StreamChunker::new(Cursor::new(&pattern_data), params);
            let mut count = 0;
            for chunk_result in chunker {
                let _chunk = black_box(chunk_result.unwrap());
                count += 1;
            }
            count
        });
    });

    // Test 3: All zeros (best case for compression)
    let zeros = vec![0u8; data_size];
    group.bench_function("Zeros", |b| {
        b.iter(|| {
            let chunker = StreamChunker::new(Cursor::new(&zeros), params);
            let mut count = 0;
            for chunk_result in chunker {
                let _chunk = black_box(chunk_result.unwrap());
                count += 1;
            }
            count
        });
    });

    // Test 4: Repeated blocks (redundant data)
    let repeated = generate_repeated(data_size);
    group.bench_function("Repeated", |b| {
        b.iter(|| {
            let chunker = StreamChunker::new(Cursor::new(&repeated), params);
            let mut count = 0;
            for chunk_result in chunker {
                let _chunk = black_box(chunk_result.unwrap());
                count += 1;
            }
            count
        });
    });

    // Test 5: Fixed-size chunking baseline (for comparison)
    group.bench_function("Fixed-size baseline", |b| {
        b.iter(|| {
            let chunk_size = 16384; // 16KB fixed chunks
            let mut count = 0;
            let mut offset = 0;
            while offset < random_data.len() {
                let end = (offset + chunk_size).min(random_data.len());
                let _chunk = black_box(&random_data[offset..end]);
                count += 1;
                offset = end;
            }
            count
        });
    });

    group.finish();
}

/// Benchmarks chunk size distribution for different FastCDC parameters.
fn bench_chunk_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("CDC-ChunkSizes");

    let data_size = 10_000_000; // 10 MB
    let test_data = generate_random(data_size);

    group.throughput(Throughput::Bytes(data_size as u64));

    // Small chunks (8KB average)
    let params_small = DedupeParams {
        f: 13,    // 2^13 = 8KB average
        m: 1024,  // 1KB minimum
        z: 32768, // 32KB maximum
        w: 48,
        v: 16,
    };

    group.bench_function("8KB-avg", |b| {
        b.iter(|| {
            let chunker = StreamChunker::new(Cursor::new(&test_data), params_small);
            let mut count = 0;
            for chunk_result in chunker {
                let _chunk = black_box(chunk_result.unwrap());
                count += 1;
            }
            count
        });
    });

    // Medium chunks (16KB average) - default
    let params_medium = DedupeParams {
        f: 14,    // 2^14 = 16KB average
        m: 2048,  // 2KB minimum
        z: 65536, // 64KB maximum
        w: 48,
        v: 16,
    };

    group.bench_function("16KB-avg", |b| {
        b.iter(|| {
            let chunker = StreamChunker::new(Cursor::new(&test_data), params_medium);
            let mut count = 0;
            for chunk_result in chunker {
                let _chunk = black_box(chunk_result.unwrap());
                count += 1;
            }
            count
        });
    });

    // Large chunks (32KB average)
    let params_large = DedupeParams {
        f: 15,     // 2^15 = 32KB average
        m: 4096,   // 4KB minimum
        z: 131072, // 128KB maximum
        w: 48,
        v: 16,
    };

    group.bench_function("32KB-avg", |b| {
        b.iter(|| {
            let chunker = StreamChunker::new(Cursor::new(&test_data), params_large);
            let mut count = 0;
            for chunk_result in chunker {
                let _chunk = black_box(chunk_result.unwrap());
                count += 1;
            }
            count
        });
    });

    group.finish();
}

criterion_group!(benches, bench_fastcdc, bench_chunk_sizes);
criterion_main!(benches);
