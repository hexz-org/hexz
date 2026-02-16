use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use hexz_core::algo::dedup::hash_table::StandardHashTable;

fn make_keys(n: usize) -> Vec<[u8; 32]> {
    (0..n)
        .map(|i| *blake3::hash(&(i as u64).to_le_bytes()).as_bytes())
        .collect()
}

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_table_insert");

    for &size in &[10_000, 100_000, 1_000_000] {
        let keys = make_keys(size);

        group.bench_with_input(BenchmarkId::new("standard", size), &keys, |b, keys| {
            b.iter(|| {
                let mut table = StandardHashTable::with_capacity(size);
                for (i, key) in keys.iter().enumerate() {
                    table.insert(*key, i as u64);
                }
            });
        });
    }

    group.finish();
}

fn bench_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_table_lookup");

    for &size in &[10_000, 100_000, 1_000_000] {
        let keys = make_keys(size);

        let mut standard = StandardHashTable::with_capacity(size);
        for (i, key) in keys.iter().enumerate() {
            standard.insert(*key, i as u64);
        }

        group.bench_with_input(BenchmarkId::new("standard", size), &keys, |b, keys| {
            b.iter(|| {
                for key in keys {
                    criterion::black_box(standard.get(key));
                }
            });
        });
    }

    group.finish();
}

fn bench_mixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_table_mixed");

    for &size in &[10_000, 100_000, 1_000_000] {
        let keys = make_keys(size);

        group.bench_with_input(BenchmarkId::new("standard", size), &keys, |b, keys| {
            b.iter(|| {
                let mut table = StandardHashTable::with_capacity(size);
                for (i, key) in keys.iter().enumerate() {
                    table.insert(*key, i as u64);
                    if i > 0 && i % 4 == 0 {
                        criterion::black_box(table.get(&keys[i / 2]));
                    }
                }
            });
        });
    }

    group.finish();
}

fn bench_high_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_table_high_load");
    let base_capacity = 100_000;

    for &load in &[0.7, 0.8, 0.9, 0.95] {
        let n = (base_capacity as f64 * load) as usize;
        let keys = make_keys(n);

        group.bench_with_input(
            BenchmarkId::new("standard_insert", format!("{:.0}pct", load * 100.0)),
            &keys,
            |b, keys| {
                b.iter(|| {
                    let mut table = StandardHashTable::with_capacity(base_capacity);
                    for (i, key) in keys.iter().enumerate() {
                        table.insert(*key, i as u64);
                    }
                });
            },
        );

        // Lookup at high load
        let mut table = StandardHashTable::with_capacity(base_capacity);
        for (i, key) in keys.iter().enumerate() {
            table.insert(*key, i as u64);
        }

        group.bench_with_input(
            BenchmarkId::new("standard_lookup", format!("{:.0}pct", load * 100.0)),
            &keys,
            |b, keys| {
                b.iter(|| {
                    for key in keys {
                        criterion::black_box(table.get(key));
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_memory(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_table_memory");

    for &size in &[10_000, 100_000, 1_000_000] {
        let keys = make_keys(size);

        group.bench_with_input(BenchmarkId::new("standard", size), &keys, |b, keys| {
            b.iter_custom(|iters| {
                let start = std::time::Instant::now();
                for _ in 0..iters {
                    let mut table = StandardHashTable::with_capacity(size);
                    for (i, key) in keys.iter().enumerate() {
                        table.insert(*key, i as u64);
                    }
                    criterion::black_box(table.memory_bytes());
                    criterion::black_box(table.stats());
                }
                start.elapsed()
            });
        });
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(std::time::Duration::from_secs(3));
    targets = bench_insert, bench_lookup, bench_mixed, bench_high_load, bench_memory
}
criterion_main!(benches);
