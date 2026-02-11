//! Compares read API strategies: Vec return (copy) vs zeroed buffer vs uninit buffer.
//!
//! **When is parallel decompression used?**
//! Only when a single read spans **2+ blocks** (see `PARALLEL_MIN_BLOCKS` in strata-core).
//! All groups read the same total bytes per iteration (4 MiB) so times and throughput are comparable.
//! - **ReadAPI_Comparison**: 64 × 64K reads (single-block each) → single-threaded path; compares alloc strategies.
//! - **ReadAPI_SingleBlockReads**: 64 × 64K reads (same total 4 MiB, one block per call) → single-threaded.
//! - **ReadAPI_MultiBlock**: one 4 MiB read (64 blocks in one call) → parallel path (Rayon).
//!
//! **Comparing 1 vs N cores:** run with `RAYON_NUM_THREADS=1` vs unset (or `RAYON_NUM_THREADS=8` etc.):
//!   RAYON_NUM_THREADS=1 cargo bench --bench read_api_comparison -- ReadAPI_MultiBlock
//!   cargo bench --bench read_api_comparison -- ReadAPI_MultiBlock

use criterion::{Criterion, SamplingMode, Throughput, black_box, criterion_group, criterion_main};
use std::io::Write;
use std::mem::MaybeUninit;
use std::sync::Arc;
use strata_cli::cmd::data::pack;
use strata_core::StrataFile;
use strata_core::algo::compression::lz4::Lz4Compressor;
use strata_core::api::stratafile::SnapshotStream;
use strata_core::store::local::FileBackend;
use tempfile::NamedTempFile;

const BLOCK_SIZE: usize = 65536;
/// Total bytes read per iteration in every group so results are comparable.
const TOTAL_BYTES_PER_ITERATION: u64 = 4 * 1024 * 1024; // 4 MiB
/// Number of 64K reads to reach TOTAL_BYTES_PER_ITERATION (single-block path).
const NUM_SINGLE_BLOCK_READS: usize = (TOTAL_BYTES_PER_ITERATION / BLOCK_SIZE as u64) as usize;

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
        65536,
        false,
        16384,
        65536,
        131072,
        true,
    )
    .unwrap();

    (input_file, output_file)
}

fn bench_read_api_comparison(c: &mut Criterion) {
    let size_mb = 50;
    let (_input, output) = setup_snapshot(size_mb);
    let output_path = output.path().to_path_buf();

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snap = StrataFile::new(backend, compressor, None).unwrap();

    // Warm cache over first 4 MiB
    for offset in (0..TOTAL_BYTES_PER_ITERATION).step_by(BLOCK_SIZE) {
        let _ = snap
            .read_at(SnapshotStream::Disk, offset, BLOCK_SIZE)
            .unwrap();
    }

    let mut group = c.benchmark_group("ReadAPI_Comparison");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(24);
    group.throughput(Throughput::Bytes(TOTAL_BYTES_PER_ITERATION));

    let num_blocks = (TOTAL_BYTES_PER_ITERATION / BLOCK_SIZE as u64) as usize;

    // 1) read_at: returns Vec<u8> (alloc + copy per read)
    group.bench_function("read_at_vec", |b| {
        let snap = snap.clone();
        b.iter(|| {
            for i in 0..NUM_SINGLE_BLOCK_READS {
                let offset = ((i * 31) % num_blocks) as u64 * BLOCK_SIZE as u64;
                let data = snap
                    .read_at(SnapshotStream::Disk, offset, BLOCK_SIZE)
                    .unwrap();
                black_box(data);
            }
        });
    });

    // 2) read_at_into: zeroed buffer [0u8; N]
    group.bench_function("read_at_into_zeroed", |b| {
        let snap = snap.clone();
        b.iter(|| {
            let mut buf = [0u8; BLOCK_SIZE];
            for i in 0..NUM_SINGLE_BLOCK_READS {
                let offset = ((i * 31) % num_blocks) as u64 * BLOCK_SIZE as u64;
                snap.read_at_into(SnapshotStream::Disk, offset, &mut buf)
                    .unwrap();
            }
            black_box(&buf);
        });
    });

    // 3) read_at_into_uninit: uninitialized buffer
    group.bench_function("read_at_into_uninit", |b| {
        let snap = snap.clone();
        b.iter(|| {
            let mut buf = [MaybeUninit::uninit(); BLOCK_SIZE];
            for i in 0..NUM_SINGLE_BLOCK_READS {
                let offset = ((i * 31) % num_blocks) as u64 * BLOCK_SIZE as u64;
                snap.read_at_into_uninit(SnapshotStream::Disk, offset, &mut buf)
                    .unwrap();
            }
            black_box(&buf);
        });
    });

    group.finish();

    // --- Same 4 MiB as single-block reads (64 × 64K) — compares alloc per read vs buffer reuse ---
    let mut single = c.benchmark_group("ReadAPI_SingleBlockReads");
    single.sampling_mode(SamplingMode::Flat);
    single.sample_size(30);
    single.throughput(Throughput::Bytes(TOTAL_BYTES_PER_ITERATION));

    // read_at (alloc per read, same total 4 MiB)
    single.bench_function("read_at_vec", |b| {
        let snap = snap.clone();
        b.iter(|| {
            for offset in (0..TOTAL_BYTES_PER_ITERATION).step_by(BLOCK_SIZE) {
                let data = snap
                    .read_at(SnapshotStream::Disk, offset, BLOCK_SIZE)
                    .unwrap();
                black_box(data);
            }
        });
    });

    // zeroed then read_at_into (alloc + zero per read)
    single.bench_function("zeroed_then_read_at_into", |b| {
        let snap = snap.clone();
        b.iter(|| {
            for offset in (0..TOTAL_BYTES_PER_ITERATION).step_by(BLOCK_SIZE) {
                let mut buf = vec![0u8; BLOCK_SIZE];
                snap.read_at_into(SnapshotStream::Disk, offset, &mut buf)
                    .unwrap();
                black_box(buf);
            }
        });
    });

    single.finish();

    // --- One 4 MiB read (64 blocks) — parallel decompression across cores ---
    rayon::join(|| (), || ()); // ensure global pool is initialized so current_num_threads() is accurate
    eprintln!(
        "ReadAPI_MultiBlock: Rayon num_threads = {} (set RAYON_NUM_THREADS to compare 1 vs N cores)",
        rayon::current_num_threads()
    );
    let mut multi = c.benchmark_group("ReadAPI_MultiBlock");
    multi.sampling_mode(SamplingMode::Flat);
    multi.sample_size(20);
    multi.throughput(Throughput::Bytes(TOTAL_BYTES_PER_ITERATION));

    multi.bench_function("read_at", |b| {
        let snap = snap.clone();
        let len = TOTAL_BYTES_PER_ITERATION as usize;
        b.iter(|| {
            let data = snap.read_at(SnapshotStream::Disk, 0, len).unwrap();
            black_box(data);
        });
    });

    multi.finish();
}

criterion_group!(benches, bench_read_api_comparison);
criterion_main!(benches);
