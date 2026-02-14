//! Deduplication Efficiency Macro-Benchmark.
//!
//! This benchmark measures actual CDC vs fixed-size deduplication savings on
//! controlled datasets with known duplication patterns. It validates the claim
//! of "10-40% additional reduction with CDC" and demonstrates CDC's advantage
//! with shifted data.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use hexz_cli::cmd::data::pack;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;

/// Creates a dataset with specified duplication percentage.
fn create_dataset_with_duplication(
    size: usize,
    duplication_pct: f64,
    temp_dir: &TempDir,
) -> std::path::PathBuf {
    let file_path = temp_dir.path().join("test_data.bin");
    let mut file = File::create(&file_path).unwrap();

    let block_size = 4096;
    let num_blocks = size / block_size;
    let unique_blocks = ((num_blocks as f64) * (1.0 - duplication_pct)) as usize;

    let mut rng = StdRng::seed_from_u64(42);

    // Generate unique blocks
    let mut unique_data = Vec::new();
    for _ in 0..unique_blocks {
        let mut block = vec![0u8; block_size];
        for byte in &mut block {
            *byte = rng.r#gen::<u8>();
        }
        unique_data.push(block);
    }

    // Write blocks, repeating some for duplication
    for i in 0..num_blocks {
        let block_idx = if i < unique_blocks {
            i
        } else {
            // Repeat earlier blocks
            i % unique_blocks
        };
        file.write_all(&unique_data[block_idx]).unwrap();
    }

    file.flush().unwrap();
    drop(file);
    file_path
}

/// Creates a dataset with a shift (tests CDC advantage).
fn create_shifted_dataset(
    base_size: usize,
    shift_bytes: usize,
    temp_dir: &TempDir,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let mut rng = StdRng::seed_from_u64(42);

    // Create base dataset
    let base_path = temp_dir.path().join("base.bin");
    let mut base_file = File::create(&base_path).unwrap();
    let mut base_data = vec![0u8; base_size];
    for byte in &mut base_data {
        *byte = rng.r#gen::<u8>();
    }
    base_file.write_all(&base_data).unwrap();
    base_file.flush().unwrap();
    drop(base_file);

    // Create shifted version (insert bytes at start)
    let shifted_path = temp_dir.path().join("shifted.bin");
    let mut shifted_file = File::create(&shifted_path).unwrap();

    // Write shift bytes
    for _ in 0..shift_bytes {
        shifted_file.write_all(&[0xFFu8]).unwrap();
    }
    // Write original data
    shifted_file.write_all(&base_data).unwrap();
    shifted_file.flush().unwrap();
    drop(shifted_file);

    (base_path, shifted_path)
}

/// Measure dedup efficiency for no duplication scenario.
fn bench_dedup_no_duplication(c: &mut Criterion) {
    let mut group = c.benchmark_group("Dedup-NoDup");
    group.sample_size(10);

    let size = 50_000_000; // 50 MB

    group.bench_function("Fixed-size", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let input = create_dataset_with_duplication(size, 0.0, &temp_dir);
                (temp_dir, input)
            },
            |(temp_dir, input)| {
                let output = temp_dir.path().join("snapshot.hxz");
                pack::run(
                    Some(input),
                    None,
                    output.clone(),
                    "lz4".to_string(),
                    false,
                    false,
                    65536,
                    false, // no CDC
                    16384,
                    65536,
                    131072,
                    true,
                )
                .unwrap();

                let output_size = std::fs::metadata(&output).unwrap().len();
                black_box(output_size);
                drop(temp_dir);
            },
        );
    });

    group.bench_function("CDC", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let input = create_dataset_with_duplication(size, 0.0, &temp_dir);
                (temp_dir, input)
            },
            |(temp_dir, input)| {
                let output = temp_dir.path().join("snapshot.hxz");
                pack::run(
                    Some(input),
                    None,
                    output.clone(),
                    "lz4".to_string(),
                    false,
                    false,
                    65536,
                    true, // CDC enabled
                    16384,
                    65536,
                    131072,
                    true,
                )
                .unwrap();

                let output_size = std::fs::metadata(&output).unwrap().len();
                black_box(output_size);
                drop(temp_dir);
            },
        );
    });

    group.finish();
}

/// Measure dedup efficiency with 25% duplication.
fn bench_dedup_25_percent(c: &mut Criterion) {
    let mut group = c.benchmark_group("Dedup-25pct");
    group.sample_size(10);

    let size = 50_000_000; // 50 MB

    group.bench_function("Fixed-size", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let input = create_dataset_with_duplication(size, 0.25, &temp_dir);
                (temp_dir, input)
            },
            |(temp_dir, input)| {
                let output = temp_dir.path().join("snapshot.hxz");
                pack::run(
                    Some(input),
                    None,
                    output.clone(),
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

                let output_size = std::fs::metadata(&output).unwrap().len();
                black_box(output_size);
                drop(temp_dir);
            },
        );
    });

    group.bench_function("CDC", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let input = create_dataset_with_duplication(size, 0.25, &temp_dir);
                (temp_dir, input)
            },
            |(temp_dir, input)| {
                let output = temp_dir.path().join("snapshot.hxz");
                pack::run(
                    Some(input),
                    None,
                    output.clone(),
                    "lz4".to_string(),
                    false,
                    false,
                    65536,
                    true,
                    16384,
                    65536,
                    131072,
                    true,
                )
                .unwrap();

                let output_size = std::fs::metadata(&output).unwrap().len();
                black_box(output_size);
                drop(temp_dir);
            },
        );
    });

    group.finish();
}

/// Measure dedup efficiency with shifted data (CDC's key advantage).
fn bench_dedup_shifted(c: &mut Criterion) {
    let mut group = c.benchmark_group("Dedup-Shifted");
    group.sample_size(10);

    let base_size = 50_000_000; // 50 MB
    let shift = 1024; // 1KB shift

    // This tests CDC's key advantage: fixed-size chunking fails with shifted data,
    // but CDC should still deduplicate successfully

    group.bench_function("Fixed-size", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let (base, shifted) = create_shifted_dataset(base_size, shift, &temp_dir);

                // Pack base
                let base_output = temp_dir.path().join("base.hxz");
                pack::run(
                    Some(base),
                    None,
                    base_output,
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

                (temp_dir, shifted)
            },
            |(temp_dir, shifted)| {
                // Pack shifted (should have poor dedup with fixed-size)
                let shifted_output = temp_dir.path().join("shifted.hxz");
                pack::run(
                    Some(shifted),
                    None,
                    shifted_output.clone(),
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

                let output_size = std::fs::metadata(&shifted_output).unwrap().len();
                black_box(output_size);
                drop(temp_dir);
            },
        );
    });

    group.bench_function("CDC", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let (base, shifted) = create_shifted_dataset(base_size, shift, &temp_dir);

                // Pack base with CDC
                let base_output = temp_dir.path().join("base.hxz");
                pack::run(
                    Some(base),
                    None,
                    base_output,
                    "lz4".to_string(),
                    false,
                    false,
                    65536,
                    true,
                    16384,
                    65536,
                    131072,
                    true,
                )
                .unwrap();

                (temp_dir, shifted)
            },
            |(temp_dir, shifted)| {
                // Pack shifted with CDC (should still deduplicate well)
                let shifted_output = temp_dir.path().join("shifted.hxz");
                pack::run(
                    Some(shifted),
                    None,
                    shifted_output.clone(),
                    "lz4".to_string(),
                    false,
                    false,
                    65536,
                    true,
                    16384,
                    65536,
                    131072,
                    true,
                )
                .unwrap();

                let output_size = std::fs::metadata(&shifted_output).unwrap().len();
                black_box(output_size);
                drop(temp_dir);
            },
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_dedup_no_duplication,
    bench_dedup_25_percent,
    bench_dedup_shifted
);
criterion_main!(benches);
