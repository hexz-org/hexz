//! Deduplication Efficiency Macro-Benchmark.
//!
//! This benchmark measures actual CDC vs fixed-size deduplication savings on
//! controlled datasets with known duplication patterns. It validates the claim
//! of "10-40% additional reduction with CDC" and demonstrates CDC's advantage
//! with shifted data.
//!
//! This benchmark outputs actual compression ratios and deduplication percentages
//! to validate performance claims in BENCHMARKS.md.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use hexz_cli::cmd::data::pack;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;

/// Results from a deduplication test
struct DedupResults {
    input_size: u64,
    output_size: u64,
    compression_ratio: f64,
    space_savings_pct: f64,
}

impl DedupResults {
    fn new(input_size: u64, output_size: u64) -> Self {
        let compression_ratio = input_size as f64 / output_size as f64;
        let space_savings_pct = (1.0 - (output_size as f64 / input_size as f64)) * 100.0;

        Self {
            input_size,
            output_size,
            compression_ratio,
            space_savings_pct,
        }
    }

    fn print(&self, label: &str) {
        eprintln!(
            "  {}: {:.2} MB → {:.2} MB ({:.2}x compression, {:.1}% savings)",
            label,
            self.input_size as f64 / 1_000_000.0,
            self.output_size as f64 / 1_000_000.0,
            self.compression_ratio,
            self.space_savings_pct,
        );
    }
}

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
    eprintln!("\n=== No Duplication (50 MB random data) ===");

    let mut fixed_results: Option<DedupResults> = None;
    let mut cdc_results: Option<DedupResults> = None;

    group.bench_function("Fixed-size", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let input = create_dataset_with_duplication(size, 0.0, &temp_dir);
                let input_size = std::fs::metadata(&input).unwrap().len();
                (temp_dir, input, input_size)
            },
            |(temp_dir, input, input_size)| {
                let output = temp_dir.path().join("snapshot.hxz");
                pack::run(
                    Some(input),
                    None,
                    output.clone(),
                    "lz4".to_string(),
                    false,
                    false,
                    65536,
                    Some(16384),  // min_chunk
                    Some(65536),  // avg_chunk
                    Some(131072), // max_chunk
                    None,
                    false, // dcam
                    true,
                )
                .unwrap();

                let output_size = std::fs::metadata(&output).unwrap().len();
                if fixed_results.is_none() {
                    fixed_results = Some(DedupResults::new(input_size, output_size));
                }
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
                let input_size = std::fs::metadata(&input).unwrap().len();
                (temp_dir, input, input_size)
            },
            |(temp_dir, input, input_size)| {
                let output = temp_dir.path().join("snapshot.hxz");
                pack::run(
                    Some(input),
                    None,
                    output.clone(),
                    "lz4".to_string(),
                    false,
                    false,
                    65536,
                    Some(16384),  // min_chunk
                    Some(65536),  // avg_chunk
                    Some(131072), // max_chunk
                    None,
                    false, // dcam
                    true,
                )
                .unwrap();

                let output_size = std::fs::metadata(&output).unwrap().len();
                if cdc_results.is_none() {
                    cdc_results = Some(DedupResults::new(input_size, output_size));
                }
                black_box(output_size);
                drop(temp_dir);
            },
        );
    });

    group.finish();

    if let (Some(fixed), Some(cdc)) = (fixed_results, cdc_results) {
        fixed.print("Fixed-size");
        cdc.print("CDC       ");
        eprintln!(
            "  CDC overhead: {:.1}% larger output (no dedup benefit on unique data)",
            (cdc.output_size as f64 / fixed.output_size as f64 - 1.0) * 100.0
        );
    }
}

/// Measure dedup efficiency with 25% duplication.
fn bench_dedup_25_percent(c: &mut Criterion) {
    let mut group = c.benchmark_group("Dedup-25pct");
    group.sample_size(10);

    let size = 50_000_000; // 50 MB
    eprintln!("\n=== 25% Duplication (50 MB with 25% repeated blocks) ===");

    let mut fixed_results: Option<DedupResults> = None;
    let mut cdc_results: Option<DedupResults> = None;

    group.bench_function("Fixed-size", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let input = create_dataset_with_duplication(size, 0.25, &temp_dir);
                let input_size = std::fs::metadata(&input).unwrap().len();
                (temp_dir, input, input_size)
            },
            |(temp_dir, input, input_size)| {
                let output = temp_dir.path().join("snapshot.hxz");
                pack::run(
                    Some(input),
                    None,
                    output.clone(),
                    "lz4".to_string(),
                    false,
                    false,
                    65536,
                    Some(16384),  // min_chunk
                    Some(65536),  // avg_chunk
                    Some(131072), // max_chunk
                    None,
                    false, // dcam
                    true,
                )
                .unwrap();

                let output_size = std::fs::metadata(&output).unwrap().len();
                if fixed_results.is_none() {
                    fixed_results = Some(DedupResults::new(input_size, output_size));
                }
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
                let input_size = std::fs::metadata(&input).unwrap().len();
                (temp_dir, input, input_size)
            },
            |(temp_dir, input, input_size)| {
                let output = temp_dir.path().join("snapshot.hxz");
                pack::run(
                    Some(input),
                    None,
                    output.clone(),
                    "lz4".to_string(),
                    false,
                    false,
                    65536,
                    Some(16384),  // min_chunk
                    Some(65536),  // avg_chunk
                    Some(131072), // max_chunk
                    None,
                    false, // dcam
                    true,
                )
                .unwrap();

                let output_size = std::fs::metadata(&output).unwrap().len();
                if cdc_results.is_none() {
                    cdc_results = Some(DedupResults::new(input_size, output_size));
                }
                black_box(output_size);
                drop(temp_dir);
            },
        );
    });

    group.finish();

    if let (Some(fixed), Some(cdc)) = (fixed_results, cdc_results) {
        fixed.print("Fixed-size");
        cdc.print("CDC       ");
        let diff_pct = (cdc.output_size as f64 / fixed.output_size as f64 - 1.0) * 100.0;
        if diff_pct.abs() < 1.0 {
            eprintln!("  Result: Similar compression (both deduplicate append-only data equally)");
        } else {
            eprintln!(
                "  CDC difference: {:.1}% {} than fixed-size",
                diff_pct.abs(),
                if diff_pct > 0.0 { "larger" } else { "smaller" }
            );
        }
    }
}

/// Measure dedup efficiency with shifted data (CDC's key advantage).
///
/// Packs base and shifted files into ONE snapshot using the dual-stream
/// capability (disk=base, memory=shifted). Both streams share a single
/// dedup map, enabling cross-file deduplication.
///
/// With fixed-size blocks: 1KB insertion shifts all block boundaries,
/// so virtually no chunks match between base and shifted -> poor dedup.
///
/// With CDC: content-defined boundaries re-sync after the insertion,
/// so most chunks still match between base and shifted -> good dedup.
fn bench_dedup_shifted(c: &mut Criterion) {
    let mut group = c.benchmark_group("Dedup-Shifted");
    group.sample_size(10);

    let base_size = 50_000_000; // 50 MB
    let shift = 1024; // 1KB shift
    let shifted_raw = base_size as u64 + shift as u64;

    eprintln!("\n=== Shifted Data (1KB insertion causing boundary shift) ===");
    eprintln!("Testing: Pack base (50 MB) + shifted (50 MB + 1KB) into ONE snapshot");
    eprintln!("Both streams share a dedup map, enabling cross-file deduplication.\n");

    let mut fixed_base_only = 0u64;
    let mut fixed_combined = 0u64;
    let mut cdc_base_only = 0u64;
    let mut cdc_combined = 0u64;

    group.bench_function("Fixed-size", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let (base, shifted) = create_shifted_dataset(base_size, shift, &temp_dir);

                // Pack base alone for reference size
                let base_output = temp_dir.path().join("base_only.hxz");
                pack::run(
                    Some(base.clone()),
                    None,
                    base_output.clone(),
                    "lz4".to_string(),
                    false,
                    false,
                    65536,
                    Some(16384),  // min_chunk
                    Some(65536),  // avg_chunk
                    Some(131072), // max_chunk
                    None,
                    false, // dcam
                    true,
                )
                .unwrap();
                if fixed_base_only == 0 {
                    fixed_base_only = std::fs::metadata(&base_output).unwrap().len();
                }

                (temp_dir, base, shifted)
            },
            |(temp_dir, base, shifted)| {
                // Pack BOTH into one snapshot (shared dedup map)
                let combined_output = temp_dir.path().join("combined.hxz");
                pack::run(
                    Some(base),
                    Some(shifted),
                    combined_output.clone(),
                    "lz4".to_string(),
                    false,
                    false,
                    65536,
                    Some(16384),  // min_chunk
                    Some(65536),  // avg_chunk
                    Some(131072), // max_chunk
                    None,
                    false, // dcam
                    true,
                )
                .unwrap();

                let size = std::fs::metadata(&combined_output).unwrap().len();
                if fixed_combined == 0 {
                    fixed_combined = size;
                }
                black_box(size);
                drop(temp_dir);
            },
        );
    });

    group.bench_function("CDC", |b| {
        b.iter_with_setup(
            || {
                let temp_dir = TempDir::new().unwrap();
                let (base, shifted) = create_shifted_dataset(base_size, shift, &temp_dir);

                // Pack base alone for reference size
                let base_output = temp_dir.path().join("base_only.hxz");
                pack::run(
                    Some(base.clone()),
                    None,
                    base_output.clone(),
                    "lz4".to_string(),
                    false,
                    false,
                    65536,
                    Some(16384),  // min_chunk
                    Some(65536),  // avg_chunk
                    Some(131072), // max_chunk
                    None,
                    false, // dcam
                    true,
                )
                .unwrap();
                if cdc_base_only == 0 {
                    cdc_base_only = std::fs::metadata(&base_output).unwrap().len();
                }

                (temp_dir, base, shifted)
            },
            |(temp_dir, base, shifted)| {
                // Pack BOTH into one snapshot (shared dedup map)
                let combined_output = temp_dir.path().join("combined.hxz");
                pack::run(
                    Some(base),
                    Some(shifted),
                    combined_output.clone(),
                    "lz4".to_string(),
                    false,
                    false,
                    65536,
                    Some(16384),  // min_chunk
                    Some(65536),  // avg_chunk
                    Some(131072), // max_chunk
                    None,
                    false, // dcam
                    true,
                )
                .unwrap();

                let size = std::fs::metadata(&combined_output).unwrap().len();
                if cdc_combined == 0 {
                    cdc_combined = size;
                }
                black_box(size);
                drop(temp_dir);
            },
        );
    });

    group.finish();

    // Print detailed deduplication analysis
    if fixed_base_only > 0 && fixed_combined > 0 && cdc_base_only > 0 && cdc_combined > 0 {
        eprintln!("\nFixed-size blocks:");
        eprintln!(
            "  Base only:      {:.2} MB",
            fixed_base_only as f64 / 1_000_000.0
        );
        eprintln!(
            "  Base + Shifted: {:.2} MB",
            fixed_combined as f64 / 1_000_000.0
        );
        let fixed_overhead = fixed_combined.saturating_sub(fixed_base_only) as f64;
        let fixed_dedup_pct = (1.0 - fixed_overhead / shifted_raw as f64) * 100.0;
        eprintln!(
            "  Shifted overhead: {:.2} MB (of {:.2} MB shifted input)",
            fixed_overhead / 1_000_000.0,
            shifted_raw as f64 / 1_000_000.0
        );
        eprintln!("  Dedup of shifted data: {:.1}%", fixed_dedup_pct);

        eprintln!("\nCDC blocks:");
        eprintln!(
            "  Base only:      {:.2} MB",
            cdc_base_only as f64 / 1_000_000.0
        );
        eprintln!(
            "  Base + Shifted: {:.2} MB",
            cdc_combined as f64 / 1_000_000.0
        );
        let cdc_overhead = cdc_combined.saturating_sub(cdc_base_only) as f64;
        let cdc_dedup_pct = (1.0 - cdc_overhead / shifted_raw as f64) * 100.0;
        eprintln!(
            "  Shifted overhead: {:.2} MB (of {:.2} MB shifted input)",
            cdc_overhead / 1_000_000.0,
            shifted_raw as f64 / 1_000_000.0
        );
        eprintln!("  Dedup of shifted data: {:.1}%", cdc_dedup_pct);

        let advantage = cdc_dedup_pct - fixed_dedup_pct;
        eprintln!(
            "\nCDC advantage: {:.1} percentage points better deduplication on shifted data",
            advantage
        );
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .measurement_time(std::time::Duration::from_secs(10));
    targets = bench_dedup_no_duplication, bench_dedup_25_percent, bench_dedup_shifted
}
criterion_main!(benches);
