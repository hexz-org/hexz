//! Parallel decompression pipeline scaling benchmark.
//!
//! This does **not** benchmark the underlying zstd/lz4 libraries; it measures
//! whether **Strata's** decompression path scales when we run it in parallel:
//! our block layout, our [`Compressor`] trait usage, Rayon dispatch, and any
//! shared state (e.g. allocator) that could limit scaling. Pre-loads compressed
//! blocks in memory (no I/O), runs decompression at a few representative thread
//! counts (1, 2, 4, 8, and all logical cores), and reports throughput (GB/s) and
//! wall time so we can confirm the pipeline isn't bottlenecked before adding
//! parallel block decompression in the reader.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use rayon::prelude::*;
use std::sync::Arc;
use strata_core::algo::compression::Compressor;
use strata_core::algo::compression::lz4::Lz4Compressor;
use strata_core::algo::compression::zstd::ZstdCompressor;

/// Block size matching typical Strata snapshot blocks (64 KiB).
const BLOCK_SIZE: usize = 64 * 1024;
/// Total decompressed size: 128 MiB to keep runs meaningful but not excessive.
const TOTAL_DECOMPRESSED_BYTES: usize = 128 * 1024 * 1024;
/// Zstd level for "heavily compressed" blocks (high ratio, CPU-bound decompress).
const ZSTD_LEVEL: i32 = 9;

/// Representative thread counts to benchmark (1, 2, 4, 8, and all cores).
/// Avoids one benchmark per core, which is excessive.
fn thread_counts() -> Vec<usize> {
    let num_cpus = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
        .max(1);
    let mut counts = vec![1, 2, 4, 8];
    if num_cpus > 8 {
        counts.push(num_cpus);
    }
    counts.retain(|&n| n <= num_cpus);
    counts.sort_unstable();
    counts.dedup();
    counts
}

/// Builds in-memory compressed blocks: deterministic data, no I/O.
/// Returns (compressed_blocks, total_decompressed_bytes).
fn prepare_compressed_blocks_zstd() -> (Vec<Vec<u8>>, u64) {
    let compressor = ZstdCompressor::new(ZSTD_LEVEL, None);
    let num_blocks = TOTAL_DECOMPRESSED_BYTES / BLOCK_SIZE;
    let mut blocks = Vec::with_capacity(num_blocks);
    for i in 0..num_blocks {
        let mut raw = vec![0u8; BLOCK_SIZE];
        for (j, b) in raw.iter_mut().enumerate() {
            *b = ((i.wrapping_mul(31).wrapping_add(j)) % 251) as u8;
        }
        let compressed = compressor.compress(&raw).expect("compress");
        blocks.push(compressed);
    }
    let total_decompressed = (num_blocks * BLOCK_SIZE) as u64;
    (blocks, total_decompressed)
}

/// LZ4 blocks for comparison (faster decompression, different scaling).
fn prepare_compressed_blocks_lz4() -> (Vec<Vec<u8>>, u64) {
    let compressor = Lz4Compressor::new();
    let num_blocks = TOTAL_DECOMPRESSED_BYTES / BLOCK_SIZE;
    let mut blocks = Vec::with_capacity(num_blocks);
    for i in 0..num_blocks {
        let mut raw = vec![0u8; BLOCK_SIZE];
        for (j, b) in raw.iter_mut().enumerate() {
            *b = ((i.wrapping_mul(31).wrapping_add(j)) % 251) as u8;
        }
        let compressed = compressor.compress(&raw).expect("compress");
        blocks.push(compressed);
    }
    let total_decompressed = (num_blocks * BLOCK_SIZE) as u64;
    (blocks, total_decompressed)
}

fn bench_decompress_scaling_zstd(c: &mut Criterion) {
    let (blocks, total_decompressed) = prepare_compressed_blocks_zstd();
    let blocks = Arc::new(blocks);
    let compressor = ZstdCompressor::new(ZSTD_LEVEL, None);

    let mut group = c.benchmark_group("decompress_scaling_zstd");
    group.throughput(Throughput::Bytes(total_decompressed));
    group.sample_size(10);

    for num_threads in thread_counts() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .expect("rayon pool");
        let blocks_clone = Arc::clone(&blocks);

        group.bench_with_input(
            BenchmarkId::new("threads", num_threads),
            &num_threads,
            |b, _| {
                b.iter(|| {
                    pool.install(|| {
                        blocks_clone.par_iter().for_each(|cblock| {
                            let _ = black_box(compressor.decompress(black_box(cblock)).unwrap());
                        });
                    });
                });
            },
        );
    }
    group.finish();
}

fn bench_decompress_scaling_lz4(c: &mut Criterion) {
    let (blocks, total_decompressed) = prepare_compressed_blocks_lz4();
    let blocks = Arc::new(blocks);
    let compressor = Lz4Compressor::new();

    let mut group = c.benchmark_group("decompress_scaling_lz4");
    group.throughput(Throughput::Bytes(total_decompressed));
    group.sample_size(10);

    for num_threads in thread_counts() {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .expect("rayon pool");
        let blocks_clone = Arc::clone(&blocks);

        group.bench_with_input(
            BenchmarkId::new("threads", num_threads),
            &num_threads,
            |b, _| {
                b.iter(|| {
                    pool.install(|| {
                        blocks_clone.par_iter().for_each(|cblock| {
                            let _ = black_box(compressor.decompress(black_box(cblock)).unwrap());
                        });
                    });
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_decompress_scaling_zstd,
    bench_decompress_scaling_lz4
);
criterion_main!(benches);
