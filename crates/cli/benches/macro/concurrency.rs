//! Concurrency benchmarks for snapshot reads.
//!
//! Measures read throughput when multiple threads access the same snapshot
//! concurrently. Uses shared helpers to build large, compressible input
//! and compares performance across thread counts.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::sync::Arc;
use std::thread;
use strata_cli::cmd::data::pack;
use strata_core::StrataFile;
use strata_core::algo::compression::lz4::Lz4Compressor;
use strata_core::api::stratafile::SnapshotStream;
use strata_core::store::local::FileBackend;
use tempfile::NamedTempFile;

/// Shared utilities for generating large, compressible input data for benchmarks.
///
/// **Architectural intent:** Centralizes test data generation so multiple benchmark
/// suites can construct comparable workloads without duplicating file-writing logic.
///
/// **Constraints:** The module is included via a relative path attribute; moving this
/// benchmark or the shared helper requires keeping the path directive in sync.
///
/// **Side effects:** Functions in `common` perform filesystem I/O to populate temporary
/// files used by the concurrency benchmarks.
#[path = "../common.rs"]
mod common;

/// Benchmarks concurrent read performance of a single `StrataFile` across multiple threads.
///
/// **Architectural intent:** Exercises the snapshot reader under parallel access by
/// spawning several threads that issue large disk reads against the same backing file,
/// approximating multi-VM or multi-process workloads.
///
/// **Constraints:** The benchmark constructs a 500 MiB snapshot and issues four
/// simultaneous 50 MiB reads from offset zero; changes to these sizes affect both
/// memory footprint and runtime. Thread count and access pattern are fixed within this
/// benchmark and do not cover all conceivable contention scenarios.
///
/// **Side effects:** Creates temporary files on disk, performs substantial sequential
/// I/O to build the snapshot, and then repeatedly performs concurrent read operations
/// during the benchmark, consuming CPU and I/O bandwidth.
fn bench_concurrent_reads(c: &mut Criterion) {
    let size = 500 * 1024 * 1024;
    let read_size = 50 * 1024 * 1024;
    let num_threads = 4;

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
    )
    .unwrap();

    let output_path = output_file.path().to_path_buf();
    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());

    let snap = Arc::new(StrataFile::new(backend, compressor, None).unwrap());

    let mut group = c.benchmark_group("concurrency_large");

    group.throughput(Throughput::Bytes((read_size * num_threads) as u64));
    group.sample_size(20);

    group.bench_function("4_threads_50mb_read", |b| {
        b.iter(|| {
            let mut handles = vec![];
            for _ in 0..num_threads {
                let fs = snap.clone();
                handles.push(thread::spawn(move || {
                    let _ = fs
                        .read_at(SnapshotStream::Disk, 0, read_size as usize)
                        .unwrap();
                }));
            }
            for h in handles {
                h.join().unwrap();
            }
        })
    });

    group.finish();
}

criterion_group!(benches, bench_concurrent_reads);
criterion_main!(benches);
