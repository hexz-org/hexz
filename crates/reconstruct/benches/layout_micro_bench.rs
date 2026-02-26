use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use hexz_core::store::StorageBackend;
use hexz_reconstruct::store::mmap::MmapBackend;
use std::io::Write;
use tempfile::NamedTempFile;

fn bench_mmap_read(c: &mut Criterion) {
    // Create a 64MB temp file with patterned data
    let mut file = NamedTempFile::new().unwrap();
    let chunk = vec![0xABu8; 65536]; // 64KB chunks
    for _ in 0..1024 {
        file.write_all(&chunk).unwrap();
    }
    file.flush().unwrap();

    let backend = MmapBackend::new(file.path()).unwrap();
    let total_len = backend.len() as usize;

    let mut group = c.benchmark_group("mmap_read_exact");

    for &read_size in &[4096, 65536, 262144, 1048576] {
        group.throughput(Throughput::Bytes(read_size as u64));
        group.bench_with_input(
            BenchmarkId::new("sequential", read_size),
            &read_size,
            |b, &size| {
                b.iter(|| {
                    let mut offset = 0u64;
                    while (offset as usize) + size <= total_len {
                        let _ = criterion::black_box(backend.read_exact(offset, size));
                        offset += size as u64;
                    }
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("single_read", read_size),
            &read_size,
            |b, &size| {
                b.iter(|| {
                    criterion::black_box(backend.read_exact(0, size).unwrap());
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_mmap_read);
criterion_main!(benches);
