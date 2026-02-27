//! Write Throughput Macro-Benchmark.
//!
//! This module measures pack operation (sequential write) performance with various
//! compression algorithms and encryption. It tests the complete write pipeline
//! from input data to packed snapshot.
//!
//! The benchmark measures:
//! - Pack throughput (MB/s) for different compression algorithms
//! - CDC chunk size impact (small vs large chunks)
//! - Encryption overhead
//! - Bottleneck identification (CPU vs I/O)

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use hexz_cli::cmd::data::pack;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a test file with deterministic compressible content.
fn create_test_file(size: usize, temp_dir: &TempDir) -> PathBuf {
    let file_path = temp_dir.path().join("test_data.bin");
    let mut file = File::create(&file_path).unwrap();

    // Generate moderately compressible data (repeating pattern with variation)
    let pattern: Vec<u8> = (0..4096).map(|i| ((i / 64) % 256) as u8).collect();
    let mut written = 0;

    while written < size {
        let to_write = (size - written).min(pattern.len());
        file.write_all(&pattern[..to_write]).unwrap();
        written += to_write;
    }

    file.flush().unwrap();
    drop(file);
    file_path
}

/// Benchmarks write throughput for LZ4 compression (baseline).
fn bench_write_lz4(c: &mut Criterion) {
    let mut group = c.benchmark_group("Write-LZ4");

    let size = 100_000_000; // 100 MB
    group.throughput(Throughput::Bytes(size as u64));
    group.sample_size(20); // Reduce sample size for macro benchmarks

    group.bench_function("100MB", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let input_file = create_test_file(size, &temp_dir);
                (temp_dir, input_file)
            },
            |(temp_dir, input_file)| {
                let output_path = temp_dir.path().join("snapshot.hxz");

                // Pack with LZ4, auto CDC
                let result = pack::run(
                    Some(input_file.clone()),
                    None, // no memory dump
                    output_path,
                    "lz4".to_string(),
                    false, // no encryption
                    false, // no dict training
                    65536, // 64KB blocks
                    None,  // min_chunk (auto)
                    None,  // avg_chunk (auto)
                    None,  // max_chunk (auto)
                    None,  // workers (auto)
                    false, // dcam
                    true,  // silent mode
                );

                black_box(&result.unwrap());
                drop(temp_dir);
            },
        );
    });

    group.finish();
}

/// Benchmarks write throughput for Zstd-3 compression.
fn bench_write_zstd3(c: &mut Criterion) {
    let mut group = c.benchmark_group("Write-Zstd3");

    let size = 100_000_000; // 100 MB
    group.throughput(Throughput::Bytes(size as u64));
    group.sample_size(20);

    group.bench_function("100MB", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let input_file = create_test_file(size, &temp_dir);
                (temp_dir, input_file)
            },
            |(temp_dir, input_file)| {
                let output_path = temp_dir.path().join("snapshot.hxz");

                // Pack with Zstd-3
                let result = pack::run(
                    Some(input_file.clone()),
                    None,
                    output_path,
                    "zstd".to_string(),
                    false,
                    false,
                    65536,
                    None,  // min_chunk (auto)
                    None,  // avg_chunk (auto)
                    None,  // max_chunk (auto)
                    None,  // workers
                    false, // dcam
                    true,  // silent
                );

                black_box(&result.unwrap());
                drop(temp_dir);
            },
        );
    });

    group.finish();
}

/// Benchmarks CDC chunk size impact: small vs large explicit chunks.
fn bench_write_cdc_chunk_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("Write-CDC");

    let size = 100_000_000; // 100 MB
    group.throughput(Throughput::Bytes(size as u64));
    group.sample_size(20);

    // Small chunks (16KB avg)
    group.bench_function("small-chunks", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let input_file = create_test_file(size, &temp_dir);
                (temp_dir, input_file)
            },
            |(temp_dir, input_file)| {
                let output_path = temp_dir.path().join("snapshot.hxz");

                let result = pack::run(
                    Some(input_file.clone()),
                    None,
                    output_path,
                    "lz4".to_string(),
                    false,
                    false,
                    65536,
                    Some(4096),  // small min
                    Some(16384), // small avg
                    Some(65536), // small max
                    None,
                    false, // dcam
                    true,
                );

                black_box(&result.unwrap());
                drop(temp_dir);
            },
        );
    });

    // Large chunks (256KB avg)
    group.bench_function("large-chunks", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let input_file = create_test_file(size, &temp_dir);
                (temp_dir, input_file)
            },
            |(temp_dir, input_file)| {
                let output_path = temp_dir.path().join("snapshot.hxz");

                let result = pack::run(
                    Some(input_file.clone()),
                    None,
                    output_path,
                    "lz4".to_string(),
                    false,
                    false,
                    65536,
                    Some(65536),   // large min
                    Some(262144),  // large avg
                    Some(1048576), // large max
                    None,
                    false, // dcam
                    true,
                );

                black_box(&result.unwrap());
                drop(temp_dir);
            },
        );
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .measurement_time(std::time::Duration::from_secs(10));
    targets = bench_write_lz4, bench_write_zstd3, bench_write_cdc_chunk_sizes
}
criterion_main!(benches);
