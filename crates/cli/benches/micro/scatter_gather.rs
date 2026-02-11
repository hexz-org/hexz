//! Multi-block scatter-gather I/O baseline (#120).
//!
//! Measures the wall-clock latency of requesting M disjoint blocks in a single
//! logical operation (serial dispatch). Establishes O(N) baseline so that when
//! concurrent fetching (#112) is implemented (tokio::join!, thread pools,
//! io_uring), the latency reduction can be quantified.

use criterion::{BenchmarkId, Criterion, SamplingMode, criterion_group, criterion_main};
use rand::SeedableRng;
use rand::seq::index::sample;
use std::io::Write;
use std::mem::MaybeUninit;
use std::sync::Arc;
use strata_cli::cmd::data::pack;
use strata_core::StrataFile;
use strata_core::algo::compression::lz4::Lz4Compressor;
use strata_core::api::stratafile::SnapshotStream;
use strata_core::store::local::FileBackend;
use tempfile::NamedTempFile;

const BLOCK_SIZE: u64 = 65536;

/// Builds a synthetic Strata pack with known layout for scatter-gather tests.
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
        BLOCK_SIZE as u32,
        false,
        16384,
        65536,
        131072,
        true,
    )
    .unwrap();

    (input_file, output_file)
}

/// Returns M distinct block indices scattered randomly across the file (deterministic seed).
fn scatter_block_indices(num_blocks: u64, m: usize, seed: u64) -> Vec<u64> {
    let n = num_blocks as usize;
    let m = m.min(n);
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let sample_indices = sample(&mut rng, n, m);
    sample_indices.iter().map(|i| i as u64).collect()
}

/// Benchmarks wall-clock latency of one scatter-gather: request M disparate blocks serially.
///
/// Each iteration performs M read_at calls (one per block) and measures total time.
/// Latency is expected to grow O(N) with current serial implementation; after #112
/// (concurrent multi-block), re-run to quantify improvement.
fn bench_scatter_gather_latency(c: &mut Criterion) {
    let size_mb = 100;
    let (_input, output) = setup_snapshot(size_mb);
    let output_path = output.path().to_path_buf();

    let backend = Arc::new(FileBackend::new(&output_path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    let snap = StrataFile::new(backend, compressor, None).unwrap();

    let stream_size = snap.size(SnapshotStream::Disk);
    let num_blocks = stream_size / BLOCK_SIZE;

    let mut group = c.benchmark_group("ScatterGather_Latency");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(24);
    group.warm_up_time(std::time::Duration::from_secs(1));
    group.measurement_time(std::time::Duration::from_secs(3));

    for num_blocks_requested in [4, 16, 64, 256] {
        let m = num_blocks_requested.min(num_blocks as usize);
        let offsets: Vec<u64> = scatter_block_indices(num_blocks, m, 42)
            .into_iter()
            .map(|bi| bi * BLOCK_SIZE)
            .collect();

        let snap_clone = snap.clone();
        group.bench_with_input(BenchmarkId::new("blocks", m), &offsets, |b, offsets| {
            b.iter(|| {
                let mut buf = [MaybeUninit::uninit(); BLOCK_SIZE as usize];
                for &offset in offsets {
                    snap_clone
                        .read_at_into_uninit(SnapshotStream::Disk, offset, &mut buf)
                        .unwrap();
                }
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_scatter_gather_latency);
criterion_main!(benches);
