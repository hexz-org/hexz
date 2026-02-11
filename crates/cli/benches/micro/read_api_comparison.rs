//! Compares read API strategies: Vec return (copy) vs zeroed buffer vs uninit buffer.
//!
//! Runs the same workload with `read_at`, `read_at_into` ([0u8; N]), and
//! `read_at_into_uninit` ([MaybeUninit::uninit(); N]) so you can see which is
//! actually fastest in one run.

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
const NUM_READS: usize = 256;
const WORKING_SET_BYTES: u64 = 2 * 1024 * 1024;

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
    let snap = Arc::new(StrataFile::new(backend, compressor, None).unwrap());

    // Warm cache
    for offset in (0..WORKING_SET_BYTES).step_by(BLOCK_SIZE) {
        let _ = snap
            .read_at(SnapshotStream::Disk, offset, BLOCK_SIZE)
            .unwrap();
    }

    let total_bytes = (NUM_READS * BLOCK_SIZE) as u64;
    let mut group = c.benchmark_group("ReadAPI_Comparison");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(24);
    group.throughput(Throughput::Bytes(total_bytes));

    // 1) read_at: returns Vec<u8> (alloc + copy per read)
    group.bench_function("read_at_vec", |b| {
        let snap = snap.clone();
        b.iter(|| {
            for i in 0..NUM_READS {
                let offset = ((i * 31) % (WORKING_SET_BYTES as usize / BLOCK_SIZE)) * BLOCK_SIZE;
                let data = snap
                    .read_at(SnapshotStream::Disk, offset as u64, BLOCK_SIZE)
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
            for i in 0..NUM_READS {
                let offset = ((i * 31) % (WORKING_SET_BYTES as usize / BLOCK_SIZE)) * BLOCK_SIZE;
                snap.read_at_into(SnapshotStream::Disk, offset as u64, &mut buf)
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
            for i in 0..NUM_READS {
                let offset = ((i * 31) % (WORKING_SET_BYTES as usize / BLOCK_SIZE)) * BLOCK_SIZE;
                snap.read_at_into_uninit(SnapshotStream::Disk, offset as u64, &mut buf)
                    .unwrap();
            }
            black_box(&buf);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_read_api_comparison);
criterion_main!(benches);
