//! Micro-benchmark: read_at allocation overhead.
//!
//! Measures the cost of read_at on cached blocks, isolating the buffer
//! allocation path (Vec::new + resize_with vs with_capacity + set_len).

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use hexz_cli::cmd::data::pack;
use hexz_core::File;
use hexz_core::api::file::SnapshotStream;
use hexz_store::local::MmapBackend;
use std::io::Write;
use std::sync::Arc;
use tempfile::NamedTempFile;

const BLOCK_SIZE: usize = 65536;

fn setup_snapshot() -> (NamedTempFile, NamedTempFile) {
    let size = 4 * 1024 * 1024; // 4MB
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
        None,
        true,
    )
    .unwrap();

    (input_file, output_file)
}

fn bench_read_at(c: &mut Criterion) {
    let (_input, output) = setup_snapshot();
    let backend = Arc::new(MmapBackend::new(output.path()).unwrap());
    let snap = File::open(backend, None).unwrap();

    // Warm cache
    for offset in (0..4 * 1024 * 1024u64).step_by(BLOCK_SIZE) {
        let _ = snap
            .read_at(SnapshotStream::Primary, offset, BLOCK_SIZE)
            .unwrap();
    }

    let mut group = c.benchmark_group("Read_Alloc");

    for read_size in [BLOCK_SIZE, 4 * BLOCK_SIZE, 16 * BLOCK_SIZE] {
        group.throughput(Throughput::Bytes(read_size as u64));
        group.bench_with_input(
            BenchmarkId::new("read_at", read_size),
            &read_size,
            |b, &size| {
                b.iter(|| {
                    black_box(snap.read_at(SnapshotStream::Primary, 0, size).unwrap());
                });
            },
        );
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(100)
        .measurement_time(std::time::Duration::from_secs(3));
    targets = bench_read_at
}
criterion_main!(benches);
