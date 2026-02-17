//! Sequential read throughput benchmarks for Hexz snapshots.
//!
//! Measures read throughput when reading a snapshot sequentially (disk or
//! secondary stream) with varying block sizes and file sizes. Uses shared
//! helpers to build large input and snapshot files.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use hexz_cli::cmd::data::pack;
use hexz_core::File;
use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_core::api::file::SnapshotStream;
use hexz_core::store::local::FileBackend;
use std::sync::Arc;
use tempfile::NamedTempFile;

/// Shared utilities for generating and writing large benchmark input files.
///
/// **Architectural intent:** Ensures that throughput benchmarks operate on consistent,
/// reproducible datasets so that changes in codec or layout can be compared in a
/// controlled fashion.
///
/// **Constraints:** Brought in via a relative path from `../common.rs`; refactoring
/// the directory layout requires updating this path.
///
/// **Side effects:** Helper functions in this module perform filesystem I/O to create
/// and populate large temporary files.
#[path = "../common.rs"]
mod common;

/// Constructs an input file and corresponding Hexz snapshot for a given size.
///
/// **Architectural intent:** Provides a reusable fixture for throughput benchmarks by
/// invoking the CLI snapshot creation path with only a primary stream, no memory image,
/// and encryption disabled, mirroring a common production configuration.
///
/// **Constraints:** The `create` command is called with `"lz4"` compression, a single
/// disk input, and encryption turned off; altering those parameters changes the codec
/// behavior and wire format and will affect benchmark results. Any failure in snapshot
/// creation causes a panic via `unwrap()`.
///
/// **Side effects:** Writes a large input file to a temporary location and generates a
/// corresponding `.hxz` file, consuming disk space and I/O bandwidth during setup.
fn setup_benchmark(size: usize) -> (NamedTempFile, NamedTempFile) {
    let input_file = NamedTempFile::new().unwrap();
    let output_file = NamedTempFile::new().unwrap();
    common::write_large_file(input_file.as_file(), size);

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
        true,   // silent
    )
    .unwrap();

    (input_file, output_file)
}

/// Benchmarks end-to-end read throughput for snapshots of varying sizes.
///
/// **Architectural intent:** Measures how read performance scales with total snapshot
/// size by constructing multiple `.hxz` files and timing sequential reads over the
/// primary stream using the standard `File` interface.
///
/// **Constraints:** The benchmark currently exercises only two sizes (100 MiB and
/// 500 MiB) and uses LZ4 compression with a single-threaded reader; it does not model
/// concurrent access or alternative codecs. Throughput is reported in bytes per second
/// for the full logical file size, not the compressed footprint.
///
/// **Side effects:** Creates multiple large temporary snapshots and repeatedly reads
/// them from disk during the benchmark run, consuming I/O bandwidth and CPU cycles.
fn bench_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput_scaling");
    group.sample_size(10);

    let sizes = [100 * 1024 * 1024, 500 * 1024 * 1024];

    for size in sizes.iter() {
        let (_input, output) = setup_benchmark(*size);
        let output_path = output.path().to_path_buf();

        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &s| {
            b.iter(|| {
                let backend = Arc::new(FileBackend::new(&output_path).unwrap());
                let compressor = Box::new(Lz4Compressor::new());
                let snap = File::new(backend, compressor, None).unwrap();
                let _ = snap.read_at(SnapshotStream::Primary, 0, s).unwrap();
            })
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .measurement_time(std::time::Duration::from_secs(10));
    targets = bench_throughput
}
criterion_main!(benches);
