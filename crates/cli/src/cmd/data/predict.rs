use anyhow::Result;
use hexz_ops::predict::{PredictConfig, predict};
use indicatif::HumanBytes;
use std::path::PathBuf;

pub fn run(path: PathBuf, block_size: u32, json: bool) -> Result<()> {
    let config = PredictConfig {
        path,
        block_size: block_size as usize,
        ..Default::default()
    };

    let report = predict(config)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("File:           {}", report.file_path);
    println!("Size:           {}", HumanBytes(report.file_size));
    println!("Block Size:     {}", HumanBytes(report.block_size as u64));
    println!("Blocks Sampled: {}", report.blocks_sampled);
    println!();

    println!("  Zero Blocks:    {:.1}%", report.zero_block_pct * 100.0);
    println!("  Mean Entropy:   {:.2} bits/byte", report.mean_entropy);
    println!(
        "  High Entropy:   {:.1}%  (incompressible)",
        report.high_entropy_pct * 100.0
    );
    println!();

    println!(
        "  LZ4:            {:.1}x  ({:.1}% savings)  -> ~{}",
        if report.lz4_ratio > 0.0 {
            1.0 / report.lz4_ratio
        } else {
            f64::INFINITY
        },
        report.lz4_savings_pct,
        HumanBytes(report.estimated_lz4_size)
    );
    println!(
        "  Zstd (level 3): {:.1}x  ({:.1}% savings)  -> ~{}",
        if report.zstd_ratio > 0.0 {
            1.0 / report.zstd_ratio
        } else {
            f64::INFINITY
        },
        report.zstd_savings_pct,
        HumanBytes(report.estimated_zstd_size)
    );
    println!();

    println!(
        "  Fixed Dedup:    {:.1}% savings",
        report.fixed_dedup_savings_pct
    );
    println!(
        "  CDC Dedup:      {:.1}% savings  (c={:.4})",
        report.cdc_baseline_savings_pct, report.cdc_change_rate
    );
    println!(
        "  CDC Optimal:    {:.1}% savings  (f={}, avg chunk ~{})",
        report.dcam_best_savings_pct,
        report.dcam_best_f,
        HumanBytes(report.dcam_best_avg_chunk as u64)
    );
    println!();

    println!(
        "  LZ4 + fixed:    {}",
        HumanBytes(report.estimated_packed_size_lz4_fixed)
    );
    println!(
        "  Zstd + CDC:     {}  ({:.1}% reduction)",
        HumanBytes(report.estimated_packed_size_zstd_cdc),
        report.overall_best_savings_pct
    );
    println!();

    // Build a recommendation based on results
    let file_path = &report.file_path;
    if report.overall_best_savings_pct > 10.0 {
        if report.dcam_best_savings_pct > 1.0 {
            println!(
                "Try: hexz pack output.hxz --disk {} --compression zstd --cdc",
                file_path
            );
        } else {
            println!(
                "Try: hexz pack output.hxz --disk {} --compression zstd",
                file_path
            );
        }
    } else if report.overall_best_savings_pct > 1.0 {
        println!(
            "Try: hexz pack output.hxz --disk {} --compression lz4",
            file_path
        );
    } else {
        println!("Data is mostly incompressible with minimal deduplication potential.");
    }

    Ok(())
}
