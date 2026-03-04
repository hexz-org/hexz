use anyhow::Result;
use hexz_ops::predict::{PredictConfig, predict};
use indicatif::HumanBytes;
use std::path::PathBuf;
use colored::*;

pub fn run(
    path: PathBuf,
    block_size: u32,
    min_chunk: Option<u32>,
    avg_chunk: Option<u32>,
    max_chunk: Option<u32>,
    json: bool,
) -> Result<()> {
    let config = PredictConfig {
        path,
        block_size: block_size as usize,
        min_chunk,
        avg_chunk,
        max_chunk,
        ..Default::default()
    };

    let report = predict(config)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("{} Prediction   {}", "╭".dimmed(), report.file_path.cyan());
    println!("{} Size         {}", "│".dimmed(), HumanBytes(report.file_size).to_string().green());
    println!("{} Block Size   {}", "╰".dimmed(), HumanBytes(report.block_size as u64).to_string().bright_black());

    println!("\n  {} Statistics:", "→".yellow());
    println!("    {} Zero Blocks    {:.1}%", "→".dimmed(), report.zero_block_pct * 100.0);
    println!("    {} Mean Entropy   {:.2} bits/byte", "→".dimmed(), report.mean_entropy);
    println!("    {} High Entropy   {:.1}%", "→".dimmed(), report.high_entropy_pct * 100.0);

    println!("\n  {} Estimation:", "→".yellow());
    println!(
        "    {} LZ4            {:.1}x  ({})",
        "→".dimmed(),
        if report.lz4_ratio > 0.0 { 1.0 / report.lz4_ratio } else { 0.0 },
        HumanBytes(report.estimated_lz4_size).to_string().bright_black()
    );
    println!(
        "    {} Zstd (lvl 3)   {:.1}x  ({})",
        "→".dimmed(),
        if report.zstd_ratio > 0.0 { 1.0 / report.zstd_ratio } else { 0.0 },
        HumanBytes(report.estimated_zstd_size).to_string().bright_black()
    );

    println!("\n  {} Deduplication:", "→".yellow());
    println!("    {} Fixed Dedup    {:.1}% savings", "→".dimmed(), report.fixed_dedup_savings_pct);
    println!("    {} CDC Dedup      {:.1}% savings", "→".dimmed(), report.cdc_dedup_savings_pct);

    println!("\n  {} Combined Best:", "→".yellow());
    println!(
        "    {} Zstd + CDC     {}  ({:.1}% reduction)",
        "→".dimmed(),
        HumanBytes(report.estimated_packed_size_zstd_cdc).to_string().green(),
        report.overall_best_savings_pct
    );

    println!("\n  {} Recommendation:", "→".yellow());
    // Recommendation logic...
    if report.overall_best_savings_pct > 10.0 {
        if report.cdc_dedup_savings_pct > 1.0 {
            println!("    {} Use {} with {} blocks and {} algorithm", "→".dimmed(), "CDC packing".cyan(), "zstd".magenta(), "zstd".magenta());
        } else {
            println!("    {} Use {} with {} algorithm", "→".dimmed(), "standard packing".cyan(), "zstd".magenta());
        }
    } else if report.overall_best_savings_pct > 1.0 {
        println!("    {} Use {} with {} algorithm", "→".dimmed(), "standard packing".cyan(), "lz4".magenta());
    } else {
        println!("    {} Data is mostly incompressible.", "→".dimmed());
    }

    Ok(())
}
