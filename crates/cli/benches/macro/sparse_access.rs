//! Sparse (random) access benchmarks for Hexz snapshots.
//!
//! Measures read throughput when issuing randomly scattered reads against
//! a snapshot. Uses a deterministic pattern and a pre-built snapshot to
//! compare cache and backend behavior under non-sequential access.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use hexz_cli::cmd::data::pack;
use hexz_core::File;
use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_core::api::file::SnapshotStream;
use hexz_store::local::FileBackend;
use std::io::Write;
use std::sync::Arc;
use tempfile::NamedTempFile;

/// Creates an input file and corresponding snapshot tailored for sparse-access tests.
///
/// **Architectural intent:** Generates a deterministic byte pattern of configurable
/// size and encodes it into a Hexz image so that randomly scattered reads can be
/// issued against a known layout without additional setup logic in each benchmark.
///
/// **Constraints:** The pattern cycles every 251 bytes; altering this logic changes
/// the compressibility and cache behavior being measured. The function assumes the
/// `create` command succeeds and will panic on failure via `unwrap()`.
///
/// **Side effects:** Writes `size` bytes of data into a temporary file and produces an
/// associated `.hxz` file, consuming disk space and I/O bandwidth during setup.
fn setup_benchmark(size: usize) -> (NamedTempFile, NamedTempFile) {
    let mut input_file = NamedTempFile::new().unwrap();
    let output_file = NamedTempFile::new().unwrap();

    let mut data = vec![0u8; size];
    for (i, byte) in data.iter_mut().enumerate() {
        *byte = (i % 251) as u8;
    }
    input_file.write_all(&data).unwrap();
    input_file.flush().unwrap();

    pack::run(
        Some(input_file.path().to_path_buf()),
        None,
        output_file.path().to_path_buf(),
        "lz4".to_string(),
        false,
        false,  // train_dict
        65536,  // block_size
        false,  // cdc_enabled
        16384,  // min_chunk
        65536,  // avg_chunk
        131072, // max_chunk
        None,   // workers
        true,   // silent
    )
    .unwrap();

    (input_file, output_file)
}

/// Benchmarks performance when reading a small number of widely separated ranges.
///
/// **Architectural intent:** Evaluates how the storage backend and snapshot index
/// behave under sparse access patterns by issuing ten 64 KiB reads spaced 10 MiB
/// apart across a 100 MiB image.
///
/// **Constraints:** The number of reads, their spacing, and the chunk size are fixed;
/// this benchmark does not explore alternative stride lengths or access distributions.
///
/// **Side effects:** Repeatedly opens and reads from the same snapshot file during the
/// benchmark, driving disk I/O and exercising any in-memory caches in the reader.
fn bench_sparse_access(c: &mut Criterion) {
    let size = 100 * 1024 * 1024;
    let (_input, output) = setup_benchmark(size);
    let output_path = output.path().to_path_buf();

    let mut group = c.benchmark_group("sparse_access");
    group.throughput(Throughput::Bytes(10 * 64 * 1024));

    group.bench_function("10_scattered_64k_reads", |b| {
        let backend = Arc::new(FileBackend::new(&output_path).unwrap());
        let compressor = Box::new(Lz4Compressor::new());
        let snap = File::new(backend, compressor, None).unwrap();

        b.iter(|| {
            for i in 0..10 {
                let offset = (i * 10 * 1024 * 1024) as u64;
                let _ = snap
                    .read_at(SnapshotStream::Primary, offset, 64 * 1024)
                    .unwrap();
            }
        });
    });

    group.finish();
}

/// Benchmarks cold and warm cache behavior for a small fixed-size read.
///
/// **Architectural intent:** Compares the latency of a 4 KiB read when performed
/// against an uncached snapshot versus a snapshot with the relevant region already
/// loaded, approximating first-touch vs subsequent page faults.
///
/// **Constraints:** Uses a 10 MiB snapshot and a fixed offset of 5 MiB; the benchmark
/// focuses solely on a single read location and does not account for broader working
/// sets. Cache behavior depends on the underlying operating system and filesystem.
///
/// **Side effects:** Constructs snapshots on disk and issues repeated reads from the
/// same offset, impacting the OS page cache and any higher-level caches used by
/// `File`.
fn bench_cache_performance(c: &mut Criterion) {
    let size = 10 * 1024 * 1024;
    let (_input, output) = setup_benchmark(size);
    let output_path = output.path().to_path_buf();

    let mut group = c.benchmark_group("cache");

    group.bench_function("cold_cache_4k", |b| {
        b.iter(|| {
            let backend = Arc::new(FileBackend::new(&output_path).unwrap());
            let compressor = Box::new(Lz4Compressor::new());
            let snap = File::new(backend, compressor, None).unwrap();
            let _ = snap
                .read_at(SnapshotStream::Primary, 5 * 1024 * 1024, 4096)
                .unwrap();
        });
    });

    group.bench_function("warm_cache_4k", |b| {
        let backend = Arc::new(FileBackend::new(&output_path).unwrap());
        let compressor = Box::new(Lz4Compressor::new());
        let snap = File::new(backend, compressor, None).unwrap();

        let _ = snap
            .read_at(SnapshotStream::Primary, 5 * 1024 * 1024, 4096)
            .unwrap();

        b.iter(|| {
            let _ = snap
                .read_at(SnapshotStream::Primary, 5 * 1024 * 1024, 4096)
                .unwrap();
        });
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .measurement_time(std::time::Duration::from_secs(10));
    targets = bench_sparse_access, bench_cache_performance
}
criterion_main!(benches);
