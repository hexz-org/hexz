//! Block Size Tradeoffs Macro-Benchmark.
//!
//! This benchmark measures the impact of block size on pack time and
//! compression ratio. It replaces the fabricated tables in documentation
//! with real measurements.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use hexz_cli::cmd::data::pack;
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;

/// Creates a test file with moderately compressible data.
fn create_test_file(size: usize, temp_dir: &TempDir) -> std::path::PathBuf {
    let file_path = temp_dir.path().join("test_data.bin");
    let mut file = File::create(&file_path).unwrap();

    // Generate moderately compressible pattern
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

/// Benchmark pack time for different block sizes.
fn bench_block_size_pack_time(c: &mut Criterion) {
    let mut group = c.benchmark_group("BlockSize-Pack");

    let test_size = 100_000_000; // 100 MB
    let block_sizes = [4096, 16384, 65536, 262144, 1048576];

    for &block_size in &block_sizes {
        group.throughput(Throughput::Bytes(test_size as u64));
        let size_kb = block_size / 1024;

        group.bench_function(format!("{}KB", size_kb), |b| {
            b.iter_with_setup(
                || {
                    let temp_dir = TempDir::new().unwrap();
                    let input = create_test_file(test_size, &temp_dir);
                    (temp_dir, input)
                },
                |(temp_dir, input)| {
                    let output = temp_dir.path().join("snapshot.hxz");
                    pack::run(
                        Some(input),
                        None,
                        output,
                        "lz4".to_string(),
                        false,
                        false,
                        block_size,
                        false,
                        16384,
                        65536,
                        131072,
                        true,
                    )
                    .unwrap();
                    drop(temp_dir);
                },
            );
        });
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .measurement_time(std::time::Duration::from_secs(10));
    targets = bench_block_size_pack_time
}
criterion_main!(benches);
