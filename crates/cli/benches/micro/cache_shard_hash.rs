//! Micro-benchmark: cache shard hash function overhead.
//!
//! Isolates the cost of shard selection hashing in `BlockCache` and
//! `ShardedPageCache`. The current implementation creates a `DefaultHasher`
//! (SipHash-1-3) per lookup; this benchmark measures the per-call overhead
//! to validate optimizations such as switching to an FxHash-style mixer.

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use hexz_core::api::file::SnapshotStream;
use hexz_core::cache::lru::BlockCache;

const CACHE_CAP: usize = 1000;
const OPS: u64 = 100_000;

/// Benchmark cache get (hit path) — exercises shard hash + LruCache internal hash.
fn bench_cache_get_hit(c: &mut Criterion) {
    let cache = BlockCache::with_capacity(CACHE_CAP);

    // Warm: insert blocks so gets are hits
    for i in 0..CACHE_CAP as u64 {
        cache.insert(
            SnapshotStream::Primary,
            i,
            bytes::Bytes::from(vec![0u8; 64]),
        );
    }

    let mut group = c.benchmark_group("Cache_Shard_Hash");
    group.throughput(Throughput::Elements(OPS));

    group.bench_function("get_hit", |b| {
        b.iter(|| {
            for i in 0..OPS {
                let block = i % (CACHE_CAP as u64);
                black_box(cache.get(SnapshotStream::Primary, block));
            }
        });
    });

    group.bench_function("insert", |b| {
        let data = bytes::Bytes::from(vec![0u8; 64]);
        b.iter(|| {
            for i in 0..OPS {
                let block = i % (CACHE_CAP as u64);
                cache.insert(SnapshotStream::Primary, block, data.clone());
            }
        });
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(std::time::Duration::from_secs(3));
    targets = bench_cache_get_hit
}
criterion_main!(benches);
