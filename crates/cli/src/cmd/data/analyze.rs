//! Analyze archive structure and optimize CDC parameters using DCAM.
//!
//! This command performs offline analysis of disk images to scientifically
//! determine optimal content-defined chunking (CDC) parameters. It uses DCAM
//! (Deduplication Change-Estimation Analytical Model), a mathematical framework
//! that predicts deduplication effectiveness without performing full chunking,
//! enabling fast parameter optimization.
//!
//! # DCAM Algorithm Overview
//!
//! DCAM estimates deduplication efficiency by:
//!
//! 1. **Baseline Pass**: Chunks a sample with LBFS parameters (8 KiB average)
//! 2. **Change Probability**: Calculates `c` (fraction of unique data)
//! 3. **Greedy Search**: Tests parameter combinations to minimize deduped size
//! 4. **Prediction**: Uses analytical model to estimate full-file deduplication
//!
//! The key insight is that deduplication effectiveness depends on:
//! - `f`: Fingerprint bits (determines average chunk size = 2^f)
//! - `m`: Minimum chunk size (prevents pathologically small chunks)
//! - `c`: Change probability (intrinsic data characteristic)
//!
//! DCAM predicts the deduplication ratio without actually deduplicating the
//! entire file, making optimization practical for large disk images.
//!
//! # Greedy Search Algorithm
//!
//! The `find_optimal_parameters` function implements a hill-climbing search:
//!
//! **Algorithm:**
//! ```text
//! current = baseline parameters (f=13, m=256)
//! best_ratio = predict_ratio(current)
//!
//! while improved:
//!   for each neighbor of current (f±1, m×2, m÷2):
//!     ratio = predict_ratio(neighbor)
//!     if ratio < best_ratio:
//!       current = neighbor
//!       best_ratio = ratio
//!       improved = true
//!   endfor
//! endwhile
//!
//! return current
//! ```
//!
//! **Search Space:**
//! - `f`: [8, 20] → average chunk size [256 B, 1 MB]
//! - `m`: [64, 16384] → minimum chunk size [64 B, 16 KiB]
//! - Constraint: `m < z` where `z = 2^(f+3)` (max chunk size)
//!
//! **Termination:**
//! - Converges when no neighbor improves the ratio
//! - Maximum 100 iterations (typically converges in 5-15)
//!
//! # Sampling Strategy
//!
//! To reduce analysis time, only 512 MiB is sampled:
//! - For files > 513 MiB: Skip first 1 MiB (to avoid partition tables/headers)
//! - For files ≤ 513 MiB: Analyze entire file
//!
//! This sampling is sufficient because deduplication characteristics are
//! typically uniform across a disk image (same filesystem, similar files).
//!
//! # Use Cases
//!
//! - **Pre-Snapshot Optimization**: Determine optimal CDC parameters before packing
//! - **Workload Characterization**: Understand data redundancy patterns
//! - **Compression Tuning**: Compare fixed vs. variable block effectiveness
//! - **Research**: Validate DCAM model predictions on real-world data
//!
//! # Recommended Workflow
//!
//! ```bash
//! # 1. Analyze disk image
//! strata analyze disk.img
//! # Output: Recommends f=14 (16 KiB avg), m=1024 (1 KiB min)
//!
//! # 2. Pack with recommended parameters
//! strata pack --disk disk.img --output snapshot.st --cdc \
//!   --min-chunk 1024 --avg-chunk 16384 --max-chunk 32768
//!
//! # 3. Verify compression ratio
//! strata info snapshot.st
//! # Output: Compression ratio should match DCAM prediction
//! ```
//!
//! # Performance Characteristics
//!
//! - **Sampling Time**: ~2-5 seconds for 512 MiB sample
//! - **Baseline Pass**: Single CDC chunking at ~200 MB/s
//! - **Greedy Search**: 10-30 iterations × DCAM prediction (~1 μs each)
//! - **Total Time**: Typically 5-10 seconds for large disk images

use anyhow::{Context, Result};
use indicatif::HumanBytes;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use strata_core::algo::dedup::cdc;
use strata_core::algo::dedup::dcam::{self, DedupeParams};

/// Size of the sample read from disk for analysis (512 MiB).
///
/// **Architectural intent:** Balances analysis accuracy with speed. Larger
/// samples improve accuracy for heterogeneous data but increase runtime.
/// 512 MiB is sufficient to characterize most workload patterns while keeping
/// analysis under 10 seconds on modern hardware.
const ANALYSIS_SAMPLE_SIZE: u64 = 512 * 1024 * 1024;

/// Executes the analyze command to optimize CDC parameters using DCAM.
///
/// Reads a sample of the input file, performs a baseline CDC chunking pass to
/// measure deduplication characteristics, calculates the change probability `c`,
/// and then uses a greedy search algorithm to find optimal chunking parameters
/// (fingerprint bits `f` and minimum chunk size `m`). Displays the baseline,
/// recommended parameters, and predicted deduplication ratio.
///
/// # Arguments
///
/// * `input` - Path to the disk image file (raw, qcow2, or any binary file)
///
/// # Output Format
///
/// ```text
/// Analyzing disk.img using DCAM...
/// Reading 512.0 MB sample for analysis...
/// Running Baseline CDC Pass (Avg Chunk: 8KB)...
///   Processed: 512.0 MB
///   Unique:    384.0 MB (75.0%)
///   Chunks:    65536
///   Estimated Change Prob (c): 0.750000
///
/// Optimizing parameters using DCAM...
///
/// --- Optimization Results ---
/// Parameter                 | Baseline (LBFS) | Recommended
/// --------------------------|-----------------|----------------
/// Fingerprint Bits (f)      | 13              | 14
/// Min Chunk Size (m)        | 256             | 1024
/// Avg Chunk Size            | 8.0 KB          | 16.0 KB
///
/// --- Predictions ---
/// Predicted Ratio: 0.7234
/// Est. Final Size: 7.2 GB
/// Est. Savings:    2.8 GB
/// ```
///
/// # Algorithm Details
///
/// The function implements these steps:
///
/// 1. **File Reading**: Opens input file and reads up to 512 MiB sample
/// 2. **Header Skipping**: For large files, skips first 1 MiB to avoid partition metadata
/// 3. **Baseline Chunking**: Runs FastCDC with LBFS parameters (f=13, m=256)
/// 4. **Statistics Collection**: Counts total bytes, unique bytes, and chunks
/// 5. **Change Probability**: Calculates `c = unique_bytes / total_bytes`
/// 6. **Greedy Optimization**: Calls `find_optimal_parameters` to search parameter space
/// 7. **Prediction**: Uses DCAM model to estimate deduplication ratio
/// 8. **Result Display**: Prints comparison table and predicted savings
///
/// # Errors
///
/// Returns an error if:
/// - Input file cannot be opened (file not found, permission denied)
/// - File metadata cannot be read
/// - File read operations fail (I/O error, disk full)
/// - CDC analysis fails (invalid data, algorithm error)
///
/// Note: Empty files are handled gracefully with an early return.
///
/// # Examples
///
/// ```no_run
/// use std::path::PathBuf;
/// use strata_cli::cmd::data::analyze;
///
/// // Analyze a disk image
/// analyze::run(PathBuf::from("vm-disk.img"))?;
///
/// // Analyze a large backup file
/// analyze::run(PathBuf::from("/backup/system.tar"))?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run(input: PathBuf) -> Result<()> {
    println!("Analyzing {} using DCAM...", input.display());

    let mut file = File::open(&input).context("Failed to open input file")?;
    let file_len = file.metadata()?.len();

    if file_len == 0 {
        println!("File is empty.");
        return Ok(());
    }

    // 1. Read Sample
    let read_len = std::cmp::min(file_len, ANALYSIS_SAMPLE_SIZE);
    let mut buffer = vec![0u8; read_len as usize];

    println!("Reading {} sample for analysis...", HumanBytes(read_len));

    // If file is large, skip the first 1MB to avoid headers/partition tables
    if file_len > ANALYSIS_SAMPLE_SIZE + 1024 * 1024 {
        file.seek(SeekFrom::Start(1024 * 1024))?;
    }
    file.read_exact(&mut buffer)?;

    // 2. Baseline Pass (LBFS)
    let baseline = DedupeParams::lbfs_baseline();
    println!("Running Baseline CDC Pass (Avg Chunk: 8KB)...");

    let stats = cdc::analyze_stream(&buffer[..], &baseline)?;

    println!("  Processed: {}", HumanBytes(stats.total_bytes));
    println!(
        "  Unique:    {} ({:.1}%)",
        HumanBytes(stats.unique_bytes),
        (stats.unique_bytes as f64 / stats.total_bytes as f64) * 100.0
    );
    println!("  Chunks:    {}", stats.chunk_count);

    // 3. Calculate c (Change Probability)
    let c = dcam::calculate_c(stats.unique_bytes, stats.total_bytes, &baseline);
    println!("  Estimated Change Prob (c): {:.6}", c);

    // 4. DCAM Greedy Search
    println!("\nOptimizing parameters using DCAM...");
    let best_params = find_optimal_parameters(stats.total_bytes, c);

    // 5. Report
    let predicted_ratio = dcam::predict_ratio(stats.total_bytes, c, &best_params);
    let predicted_size = (file_len as f64 * predicted_ratio) as u64;

    println!("\n--- Optimization Results ---");
    println!(
        "{:<25} | {:<15} | {:<15}",
        "Parameter", "Baseline (LBFS)", "Recommended"
    );
    println!("{:-<26}|{:-<16}|{:-<16}", "", "", "");

    println!(
        "{:<25} | {:<15} | {:<15}",
        "Fingerprint Bits (f)", baseline.f, best_params.f
    );
    println!(
        "{:<25} | {:<15} | {:<15}",
        "Min Chunk Size (m)", baseline.m, best_params.m
    );
    println!(
        "{:<25} | {:<15} | {:<15}",
        "Avg Chunk Size",
        HumanBytes(1 << baseline.f),
        HumanBytes(1 << best_params.f)
    );

    println!("\n--- Predictions ---");
    println!("Predicted Ratio: {:.4}", predicted_ratio);
    println!("Est. Final Size: {}", HumanBytes(predicted_size));
    println!(
        "Est. Savings:    {}",
        HumanBytes(file_len.saturating_sub(predicted_size))
    );

    Ok(())
}

/// Greedy search algorithm to find optimal fingerprint bits and minimum chunk size.
///
/// Performs hill-climbing optimization in the (f, m) parameter space to minimize
/// the predicted deduplication ratio. Starts from the LBFS baseline and explores
/// neighbors (f±1, m×2, m÷2) until no improvement is found.
///
/// # Algorithm
///
/// The search uses a simple greedy strategy:
/// 1. Start with current best parameters (initially LBFS baseline)
/// 2. Evaluate all valid neighbors in the parameter space
/// 3. Move to the neighbor with the best (lowest) predicted ratio
/// 4. Repeat until no neighbor improves the ratio
///
/// # Search Space Constraints
///
/// - **Fingerprint bits (f)**: [8, 20]
///   - f=8 → 256 B average chunk (too small, high overhead)
///   - f=20 → 1 MB average chunk (too large, poor deduplication)
///
/// - **Minimum chunk size (m)**: [64, 16384]
///   - m=64 → allows very small chunks (high metadata overhead)
///   - m=16384 → prevents most chunks (degrades to fixed-size)
///
/// - **Constraint**: m < z where z = 2^(f+3) (max chunk size)
///
/// # Convergence
///
/// Typically converges in 5-15 iterations. The maximum iteration limit (100)
/// prevents infinite loops on pathological inputs.
///
/// # Arguments
///
/// * `file_size` - Total size of the file being analyzed (used by DCAM model)
/// * `c` - Change probability (fraction of unique data after baseline pass)
///
/// # Returns
///
/// Optimal `DedupeParams` with fields:
/// - `f`: Fingerprint bits (determines average chunk size)
/// - `m`: Minimum chunk size in bytes
/// - `z`: Maximum chunk size in bytes (derived from f)
///
/// # Performance
///
/// Each iteration performs O(4) DCAM predictions (~1 μs each), so total
/// search time is typically <1 ms even for worst-case convergence.
fn find_optimal_parameters(file_size: u64, c: f64) -> DedupeParams {
    let mut current = DedupeParams::lbfs_baseline();
    let mut best_ratio = dcam::predict_ratio(file_size, c, &current);

    // Search bounds
    let min_f = 8; // 256B
    let max_f = 20; // 1MB
    let min_m = 64;
    let max_m = 16 * 1024;

    let mut improved = true;
    let mut iterations = 0;

    while improved && iterations < 100 {
        improved = false;
        iterations += 1;

        let mut best_neighbor = current;

        // 1. Explore f neighbors (f-1, f+1)
        for f_cand in [current.f.saturating_sub(1), current.f + 1] {
            if f_cand < min_f || f_cand > max_f {
                continue;
            }

            let mut candidate = current;
            candidate.f = f_cand;
            candidate.z = 1 << (f_cand + 3);

            let ratio = dcam::predict_ratio(file_size, c, &candidate);
            if ratio < best_ratio {
                best_ratio = ratio;
                best_neighbor = candidate;
                improved = true;
            }
        }

        // 2. Explore m neighbors (m/2, m*2)
        for m_cand in [current.m / 2, current.m * 2] {
            if m_cand < min_m || m_cand > max_m || m_cand >= current.z {
                continue;
            }

            let mut candidate = current;
            candidate.m = m_cand;

            let ratio = dcam::predict_ratio(file_size, c, &candidate);
            if ratio < best_ratio {
                best_ratio = ratio;
                best_neighbor = candidate;
                improved = true;
            }
        }

        current = best_neighbor;
    }

    current
}
