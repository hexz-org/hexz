//! Analyze archive structure.
//!
//! Offline analysis of snapshot geometry and block-size recommendations using
//! DCAM (Deduplication Change-Estimation Analytical Model) to scientifically
//! optimize CDC parameters.

use anyhow::{Context, Result};
use indicatif::HumanBytes;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use strata_core::algo::dedup::cdc;
use strata_core::algo::dedup::dcam::{self, DedupeParams};

/// Size of the sample read from disk for analysis (512 MiB).
const ANALYSIS_SAMPLE_SIZE: u64 = 512 * 1024 * 1024;

/// Execute the analyze command.
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

/// Greedy search algorithm to find optimal f and m.
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
