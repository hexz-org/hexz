//! Hashing Algorithm Micro-Benchmarks.
//!
//! This module measures the raw performance of cryptographic hash functions
//! used for content-defined chunking (CDC) and deduplication. It compares
//! BLAKE3 against SHA-256 to validate the performance improvements from
//! switching hash algorithms.
//!
//! The benchmarks measure throughput across different data sizes typical
//! of chunking operations, from 4KB to 1MB blocks.

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};

/// Benchmarks hashing performance for BLAKE3 vs SHA-256.
///
/// This function measures raw hashing throughput for different block sizes
/// to demonstrate the performance difference between BLAKE3 (used for CDC
/// in current implementation) and SHA-256 (previously used).
///
/// Test data sizes:
/// - 4 KB: Minimum FastCDC chunk size
/// - 64 KB: Average FastCDC chunk size
/// - 256 KB: Maximum FastCDC chunk size
/// - 1 MB: Large block size for fixed chunking
///
/// # Arguments
///
/// * `c` - The Criterion benchmark context.
fn bench_hashing(c: &mut Criterion) {
    let sizes = [
        (4 * 1024, "4KB"),
        (64 * 1024, "64KB"),
        (256 * 1024, "256KB"),
        (1024 * 1024, "1MB"),
    ];

    for (size, name) in sizes.iter() {
        let mut group = c.benchmark_group(format!("Hash/{}", name));
        group.throughput(Throughput::Bytes(*size as u64));

        // Generate deterministic test data
        let mut data = Vec::with_capacity(*size);
        for i in 0..*size {
            data.push((i % 251) as u8);
        }

        // BLAKE3 benchmark (current implementation)
        group.bench_function("BLAKE3", |b| {
            b.iter(|| {
                let _hash = blake3::hash(black_box(&data));
            })
        });

        // SHA-256 benchmark (previous implementation)
        group.bench_function("SHA-256", |b| {
            use sha2::{Digest, Sha256};
            b.iter(|| {
                let _hash = Sha256::digest(black_box(&data));
            })
        });

        group.finish();
    }
}

/// Benchmarks end-to-end deduplication performance.
///
/// This measures the complete deduplication workflow including:
/// 1. Hashing the block data
/// 2. HashMap lookup for existing blocks
/// 3. Inserting new unique blocks
///
/// This shows the real-world impact of hash function performance
/// on the deduplication subsystem.
fn bench_dedup_workflow(c: &mut Criterion) {
    use std::collections::HashMap;

    let mut group = c.benchmark_group("Deduplication");
    let block_size = 64 * 1024; // Typical chunk size
    group.throughput(Throughput::Bytes(block_size as u64));

    // Generate test blocks with varying content
    let num_blocks = 100;
    let mut blocks = Vec::new();
    for i in 0..num_blocks {
        let mut block = Vec::with_capacity(block_size);
        for j in 0..block_size {
            // Create some duplicates (every 10th block is identical)
            block.push(((i / 10 + j) % 251) as u8);
        }
        blocks.push(block);
    }

    // BLAKE3 deduplication workflow
    group.bench_function("BLAKE3 workflow", |b| {
        b.iter(|| {
            let mut dedup_map: HashMap<[u8; 32], u64> = HashMap::new();
            let mut offset = 0u64;
            let mut duplicate_count = 0;

            for block in &blocks {
                let hash: [u8; 32] = blake3::hash(black_box(block)).into();

                match dedup_map.entry(hash) {
                    std::collections::hash_map::Entry::Occupied(_) => {
                        duplicate_count += 1;
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(offset);
                        offset += block.len() as u64;
                    }
                }
            }

            black_box((duplicate_count, dedup_map.len()));
        })
    });

    // SHA-256 deduplication workflow
    group.bench_function("SHA-256 workflow", |b| {
        use sha2::{Digest, Sha256};

        b.iter(|| {
            let mut dedup_map: HashMap<[u8; 32], u64> = HashMap::new();
            let mut offset = 0u64;
            let mut duplicate_count = 0;

            for block in &blocks {
                let hash: [u8; 32] = Sha256::digest(black_box(block)).into();

                match dedup_map.entry(hash) {
                    std::collections::hash_map::Entry::Occupied(_) => {
                        duplicate_count += 1;
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(offset);
                        offset += block.len() as u64;
                    }
                }
            }

            black_box((duplicate_count, dedup_map.len()));
        })
    });

    group.finish();
}

criterion_group!(benches, bench_hashing, bench_dedup_workflow);
criterion_main!(benches);
