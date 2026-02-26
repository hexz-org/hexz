//! Compression Algorithm Micro-Benchmarks.
//!
//! This module measures the raw performance of compression and decompression
//! operations for different codec implementations. It generates deterministic
//! test data and measures throughput for both LZ4 and Zstd algorithms to
//! guide codec selection based on workload characteristics.

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use hexz_core::algo::compression::Compressor;
use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_core::algo::compression::zstd::ZstdCompressor;

/// Benchmarks compression and decompression performance for available codecs.
///
/// This function generates a 1MB test pattern, measures compression throughput
/// for LZ4 and Zstd-3, and measures decompression throughput for LZ4. The results
/// help determine which codec provides the best balance of compression ratio
/// and decompression speed for filesystem workloads where read latency is critical.
///
/// # Arguments
///
/// * `c` - The Criterion benchmark context.
fn bench_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("Compression");

    let data_size = 1024 * 1024;
    let mut data = Vec::with_capacity(data_size);
    for i in 0..data_size {
        data.push((i % 251) as u8);
    }

    group.throughput(Throughput::Bytes(data_size as u64));

    group.bench_function("LZ4 Compress", |b| {
        let compressor = Lz4Compressor::new();
        b.iter(|| {
            compressor.compress(black_box(&data)).unwrap();
        })
    });

    group.bench_function("Zstd-3 Compress", |b| {
        let compressor = ZstdCompressor::new(3, None);
        b.iter(|| {
            compressor.compress(black_box(&data)).unwrap();
        })
    });

    let lz4 = Lz4Compressor::new();
    let lz4_compressed = lz4.compress(&data).unwrap();

    group.bench_function("LZ4 Decompress", |b| {
        b.iter(|| {
            lz4.decompress(black_box(&lz4_compressed)).unwrap();
        })
    });

    let zstd = ZstdCompressor::new(3, None);
    let zstd_compressed = zstd.compress(&data).unwrap();

    group.bench_function("Zstd-3 Decompress", |b| {
        b.iter(|| {
            zstd.decompress(black_box(&zstd_compressed)).unwrap();
        })
    });

    group.bench_function("Zstd-9 Compress", |b| {
        let compressor = ZstdCompressor::new(9, None);
        b.iter(|| {
            compressor.compress(black_box(&data)).unwrap();
        })
    });

    let zstd9 = ZstdCompressor::new(9, None);
    let zstd9_compressed = zstd9.compress(&data).unwrap();

    group.bench_function("Zstd-9 Decompress", |b| {
        b.iter(|| {
            zstd9.decompress(black_box(&zstd9_compressed)).unwrap();
        })
    });

    // Dynamic dispatch (Box<dyn Compressor>) decompress benchmarks
    let dyn_lz4: Box<dyn Compressor> = Box::new(Lz4Compressor::new());
    group.bench_function("LZ4 Decompress (dyn)", |b| {
        b.iter(|| {
            dyn_lz4.decompress(black_box(&lz4_compressed)).unwrap();
        })
    });

    let dyn_zstd: Box<dyn Compressor> = Box::new(ZstdCompressor::new(3, None));
    group.bench_function("Zstd-3 Decompress (dyn)", |b| {
        b.iter(|| {
            dyn_zstd.decompress(black_box(&zstd_compressed)).unwrap();
        })
    });

    // compress_into benchmarks: measures buffer reuse effectiveness
    group.bench_function("Zstd-3 compress_into", |b| {
        let compressor = ZstdCompressor::new(3, None);
        let mut buf = Vec::with_capacity(data_size);
        b.iter(|| {
            compressor
                .compress_into(black_box(&data), &mut buf)
                .unwrap();
            black_box(&buf);
        })
    });

    group.bench_function("LZ4 compress_into", |b| {
        let compressor = Lz4Compressor::new();
        let mut buf = Vec::with_capacity(data_size);
        b.iter(|| {
            compressor
                .compress_into(black_box(&data), &mut buf)
                .unwrap();
            black_box(&buf);
        })
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(std::time::Duration::from_secs(3));
    targets = bench_compression
}
criterion_main!(benches);
