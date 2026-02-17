//! Page-cache lock contention micro-benchmarks.
//!
//! Targeted benchmarks to isolate the overhead of the page/block cache locking
//! strategy before addressing global Mutex (#114) and lock-held-across-I/O (#111).
//! ML and DataLoader benchmarks are too high-level to cleanly measure raw lock overhead.
//!
//! - **Concurrent Read Hit**: N threads repeatedly read the same subset of blocks
//!   already in cache, isolating read-lock acquisition overhead.
//! - **Concurrent Read Miss**: N threads simultaneously request distinct, uncached
//!   blocks to highlight latency penalty of holding a global lock during I/O and
//!   deserialization.
//!
//! **Throughput:** We report *reads per second* (elem/s). Each read is one 64 KiB block.
//! Logical MB/s = reads/s × 64/1024. Read Hit yields very high reads/s (and thus
//! huge GiB/s if reported as bytes) because each "read" is just lock + cache lookup +
//! copy—not actual I/O or memory bandwidth.
//!
//! **Criterion "change" / "regressed":** Those lines compare this run to a *saved baseline*
//! in `target/criterion/`. If you switched code (e.g. read_at → read_at_into_uninit) or
//! the baseline is from another machine, ignore "change" or re-save: e.g.
//! `make bench cache_concurrent -- --save-baseline my_baseline`.

use criterion::{
    BenchmarkId, Criterion, SamplingMode, Throughput, black_box, criterion_group, criterion_main,
};
use hexz_cli::cmd::data::pack;
use hexz_core::File;
use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_core::api::file::SnapshotStream;
use hexz_core::store::local::FileBackend;
use std::io::Write;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::thread;
use tempfile::NamedTempFile;

const BLOCK_SIZE: usize = 65536;
/// Working set size for read-hit: small enough to stay in L1 + page cache.
const WORKING_SET_BYTES: u64 = 2 * 1024 * 1024;
/// Reads per thread per iteration for hit benchmark (sustained contention).
const READS_PER_THREAD_HIT: usize = 128;
/// Single read size (one block).
const READ_SIZE: usize = BLOCK_SIZE;

/// Builds a Hexz snapshot for cache benchmarks (deterministic, compressible data).
fn setup_snapshot(size_mb: usize) -> (NamedTempFile, NamedTempFile) {
    let size = size_mb * 1024 * 1024;
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
        false,
        65536, // block_size
        false,
        16384,
        65536,
        131072,
        true,
    )
    .unwrap();

    (input_file, output_file)
}

// --- Concurrent Read Hit -----------------------------------------------------

/// Spawns N threads that continuously request the same subset of blocks already
/// in the cache to isolate the overhead of acquiring the read lock (and shard
/// mutex on BlockCache / page_cache).
fn bench_concurrent_read_hit(c: &mut Criterion) {
    let size_mb = 50;
    let (_input, output) = setup_snapshot(size_mb);
    let output_path = output.path().to_path_buf();

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snap = File::new(backend, compressor, None).unwrap();

    // Warm cache: read working set so all subsequent reads hit L1 and page cache.
    for offset in (0..WORKING_SET_BYTES).step_by(READ_SIZE) {
        let _ = snap
            .read_at(SnapshotStream::Primary, offset, READ_SIZE)
            .unwrap();
    }

    let mut group = c.benchmark_group("Cache_Concurrent_Read_Hit");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(24);
    group.warm_up_time(std::time::Duration::from_secs(2));
    group.measurement_time(std::time::Duration::from_secs(3));

    for num_threads in [2, 4, 8] {
        let reads_per_iter = (num_threads * READS_PER_THREAD_HIT) as u64;
        group.throughput(Throughput::Elements(reads_per_iter));

        group.bench_with_input(
            BenchmarkId::new("threads", num_threads),
            &num_threads,
            |b, &n| {
                b.iter(|| {
                    let mut handles = Vec::with_capacity(n);
                    for _ in 0..n {
                        let fs = snap.clone();
                        handles.push(thread::spawn(move || {
                            let mut buf = [MaybeUninit::uninit(); READ_SIZE];
                            for i in 0..READS_PER_THREAD_HIT {
                                let offset = ((i * 31) % (WORKING_SET_BYTES as usize / READ_SIZE))
                                    * READ_SIZE;
                                fs.read_at_into_uninit(
                                    SnapshotStream::Primary,
                                    offset as u64,
                                    &mut buf,
                                )
                                .unwrap();
                            }
                            black_box(&buf);
                        }));
                    }
                    for h in handles {
                        h.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

// --- Concurrent Read Miss ----------------------------------------------------

/// Spawns N threads that simultaneously request entirely distinct, uncached
/// blocks to highlight the latency penalty of holding a global lock while
/// blocking on I/O and deserialization.
///
/// Each iteration issues one read per thread to a distinct offset (stride-separated).
/// Early iterations are true cache misses; later iterations may hit cache if
/// the stride pattern reuses blocks. Throughput and latency distributions
/// (mean/median from Criterion) isolate lock contention under cold I/O.
fn bench_concurrent_read_miss(c: &mut Criterion) {
    let size_mb = 50;
    let (_input, output) = setup_snapshot(size_mb);
    let output_path = output.path().to_path_buf();

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snap = File::new(backend, compressor, None).unwrap();

    let stream_size = snap.size(SnapshotStream::Primary);
    // Stride so each thread hits different blocks and ideally different index pages.
    let stride = (stream_size / 32).max(2 * 1024 * 1024);

    let mut group = c.benchmark_group("Cache_Concurrent_Read_Miss");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(20);
    group.warm_up_time(std::time::Duration::from_secs(1));
    group.measurement_time(std::time::Duration::from_secs(4));

    for num_threads in [2, 4, 8] {
        let reads_per_iter = num_threads as u64;
        group.throughput(Throughput::Elements(reads_per_iter));

        group.bench_with_input(
            BenchmarkId::new("threads", num_threads),
            &num_threads,
            |b, &n| {
                b.iter(|| {
                    let mut handles = Vec::with_capacity(n);
                    for t in 0..n {
                        let fs = snap.clone();
                        let offset = (t as u64)
                            .saturating_mul(stride)
                            .min(stream_size.saturating_sub(READ_SIZE as u64));
                        handles.push(thread::spawn(move || {
                            let mut buf = [MaybeUninit::uninit(); READ_SIZE];
                            fs.read_at_into_uninit(SnapshotStream::Primary, offset, &mut buf)
                                .unwrap();
                            black_box(&buf);
                        }));
                    }
                    for h in handles {
                        h.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(std::time::Duration::from_secs(3));
    targets = bench_concurrent_read_hit, bench_concurrent_read_miss
}
criterion_main!(benches);
