//! AES-256-GCM Encryption Micro-Benchmark.
//!
//! This module measures the raw performance of AES-256-GCM authenticated encryption
//! and decryption for different block sizes. It validates the claimed "1-2 GB/s"
//! throughput and helps understand encryption overhead in the pack/unpack pipeline.
//!
//! The benchmark measures:
//! - Encryption throughput for various block sizes (4KB to 1MB)
//! - Decryption throughput (should be symmetric with encryption)
//! - AES-NI hardware acceleration effectiveness

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use hexz_core::algo::encryption::{AesGcmEncryptor, Encryptor};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// Helper to create an encryptor with a deterministic test key.
fn create_test_encryptor() -> AesGcmEncryptor {
    // Use deterministic parameters for reproducible benchmarks
    let password = b"benchmark_password";
    let salt = b"fixed_salt_16byt"; // 16 bytes for determinism
    let iterations = 10000; // Reduced iterations for faster setup (not security-critical in benchmarks)

    AesGcmEncryptor::new(password, salt, iterations).unwrap()
}

/// Generates random test data for encryption benchmarks.
fn generate_test_data(size: usize) -> Vec<u8> {
    let mut rng = StdRng::seed_from_u64(42);
    let mut data = Vec::with_capacity(size);
    for _ in 0..size {
        data.push(rng.r#gen::<u8>());
    }
    data
}

/// Benchmarks encryption performance across different block sizes.
fn bench_encryption(c: &mut Criterion) {
    let mut group = c.benchmark_group("Encryption");

    let encryptor = create_test_encryptor();
    let sizes = [4096, 16384, 65536, 262144, 1048576]; // 4KB, 16KB, 64KB, 256KB, 1MB

    for &size in &sizes {
        let data = generate_test_data(size);
        group.throughput(Throughput::Bytes(size as u64));

        let size_label = if size < 1024 {
            format!("{}B", size)
        } else if size < 1024 * 1024 {
            format!("{}KB", size / 1024)
        } else {
            format!("{}MB", size / (1024 * 1024))
        };

        group.bench_function(format!("Encrypt-{}", size_label), |b| {
            b.iter(|| {
                let block_idx = 0u64; // Fixed index for benchmark
                black_box(encryptor.encrypt(&data, block_idx).unwrap())
            });
        });
    }

    group.finish();
}

/// Benchmarks decryption performance across different block sizes.
fn bench_decryption(c: &mut Criterion) {
    let mut group = c.benchmark_group("Decryption");

    let encryptor = create_test_encryptor();
    let sizes = [4096, 16384, 65536, 262144, 1048576]; // 4KB, 16KB, 64KB, 256KB, 1MB

    for &size in &sizes {
        let data = generate_test_data(size);
        let encrypted = encryptor.encrypt(&data, 0).unwrap();
        group.throughput(Throughput::Bytes(size as u64));

        let size_label = if size < 1024 {
            format!("{}B", size)
        } else if size < 1024 * 1024 {
            format!("{}KB", size / 1024)
        } else {
            format!("{}MB", size / (1024 * 1024))
        };

        group.bench_function(format!("Decrypt-{}", size_label), |b| {
            b.iter(|| {
                let block_idx = 0u64;
                black_box(encryptor.decrypt(&encrypted, block_idx).unwrap())
            });
        });
    }

    group.finish();
}

/// Benchmarks the full encrypt/decrypt round-trip.
fn bench_roundtrip(c: &mut Criterion) {
    let mut group = c.benchmark_group("Encryption-Roundtrip");

    let encryptor = create_test_encryptor();
    let size = 65536; // 64KB - typical block size
    let data = generate_test_data(size);
    group.throughput(Throughput::Bytes(size as u64));

    group.bench_function("64KB", |b| {
        b.iter(|| {
            let block_idx = 0u64;
            let encrypted = encryptor.encrypt(&data, block_idx).unwrap();
            let decrypted = encryptor.decrypt(&encrypted, block_idx).unwrap();
            black_box(decrypted)
        });
    });

    group.finish();
}

criterion_group!(benches, bench_encryption, bench_decryption, bench_roundtrip);
criterion_main!(benches);
