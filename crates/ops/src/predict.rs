use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use hexz_common::{Error, Result};
use serde::Serialize;

use hexz_core::algo::compression::Compressor;
use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_core::algo::compression::zstd::ZstdCompressor;
use hexz_core::algo::dedup::cdc::analyze_stream;
use hexz_core::algo::dedup::dcam::{DedupeParams, optimize_params};

use crate::pack::calculate_entropy;
use crate::write::is_zero_chunk;

/// Configuration for the predict command.
#[derive(Debug, Clone)]
pub struct PredictConfig {
    /// Path to the raw data file to analyze.
    pub path: PathBuf,
    /// Block size in bytes for fixed-chunk analysis.
    pub block_size: usize,
    /// CDC minimum chunk size (auto-detected if None).
    pub min_chunk: Option<u32>,
    /// CDC average chunk size (auto-detected if None).
    pub avg_chunk: Option<u32>,
    /// CDC maximum chunk size (auto-detected if None).
    pub max_chunk: Option<u32>,
    /// Number of evenly-spaced blocks to sample for compression estimates.
    pub sample_count: usize,
    /// Max bytes to feed to `analyze_stream` for CDC analysis.
    pub dedup_scan_limit: u64,
}

impl Default for PredictConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            block_size: 65536,
            min_chunk: None,
            avg_chunk: None,
            max_chunk: None,
            sample_count: 4000,
            dedup_scan_limit: 256 * 1024 * 1024,
        }
    }
}

/// Results from analyzing a raw file for hexz packing potential.
#[derive(Debug, Serialize)]
pub struct PredictReport {
    pub file_path: String,
    pub file_size: u64,
    pub block_size: usize,
    pub blocks_sampled: usize,

    // Data characteristics
    pub zero_block_pct: f64,
    pub mean_entropy: f64,
    pub high_entropy_pct: f64,

    // Compression estimates
    pub lz4_ratio: f64,
    pub lz4_savings_pct: f64,
    pub zstd_ratio: f64,
    pub zstd_savings_pct: f64,
    pub estimated_lz4_size: u64,
    pub estimated_zstd_size: u64,

    // Fixed-size dedup (from sampled blocks)
    pub fixed_dedup_ratio: f64,
    pub fixed_dedup_savings_pct: f64,

    // CDC dedup (from analyze_stream with user's params)
    pub cdc_scan_bytes: u64,
    pub cdc_min_chunk: u32,
    pub cdc_avg_chunk: u32,
    pub cdc_max_chunk: u32,
    pub cdc_chunks_total: u64,
    pub cdc_chunks_unique: u64,
    pub cdc_dedup_ratio: f64,
    pub cdc_dedup_savings_pct: f64,

    // Combined estimates
    pub estimated_packed_size_lz4_fixed: u64,
    pub estimated_packed_size_zstd_cdc: u64,
    pub overall_best_savings_pct: f64,
}

/// Analyze a raw data file and estimate hexz packing savings.
pub fn predict(config: PredictConfig) -> Result<PredictReport> {
    let mut f = File::open(&config.path)?;
    let file_size = f.metadata()?.len();

    if file_size == 0 {
        return Err(Error::Format("File is empty".to_string()));
    }

    // Phase 1: Sample evenly-spaced blocks for compression + entropy estimates
    let step = (file_size / config.sample_count as u64).max(config.block_size as u64);
    let mut buf = vec![0u8; config.block_size];

    let lz4 = Lz4Compressor::new();
    let zstd = ZstdCompressor::new(3, None);

    let mut blocks_sampled: usize = 0;
    let mut zero_count: usize = 0;
    let mut entropy_sum: f64 = 0.0;
    let mut high_entropy_count: usize = 0;
    let mut lz4_compressed_total: u64 = 0;
    let mut zstd_compressed_total: u64 = 0;
    let mut raw_sampled_total: u64 = 0;

    // Fixed dedup tracking via blake3 hash set
    let mut seen_hashes: HashSet<u64> = HashSet::new();
    let mut unique_sampled_bytes: u64 = 0;

    let mut attempt: u64 = 0;
    while blocks_sampled < config.sample_count {
        let offset = attempt * step;
        if offset >= file_size {
            break;
        }

        f.seek(SeekFrom::Start(offset))?;
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        blocks_sampled += 1;
        raw_sampled_total += n as u64;

        // Dedup tracking for all blocks (including zeros)
        let digest = *blake3::hash(chunk).as_bytes();
        let hash = u64::from_le_bytes(digest[..8].try_into().unwrap());
        if seen_hashes.insert(hash) {
            unique_sampled_bytes += n as u64;
        }

        if is_zero_chunk(chunk) {
            zero_count += 1;
            attempt += 1;
            continue;
        }

        let entropy = calculate_entropy(chunk);
        entropy_sum += entropy;
        if entropy > 6.0 {
            high_entropy_count += 1;
        }

        // Compression measurement
        if let Ok(compressed) = lz4.compress(chunk) {
            lz4_compressed_total += compressed.len() as u64;
        } else {
            lz4_compressed_total += n as u64;
        }
        if let Ok(compressed) = zstd.compress(chunk) {
            zstd_compressed_total += compressed.len() as u64;
        } else {
            zstd_compressed_total += n as u64;
        }

        attempt += 1;
    }

    let non_zero_count = blocks_sampled - zero_count;

    let zero_block_pct = if blocks_sampled > 0 {
        zero_count as f64 / blocks_sampled as f64
    } else {
        0.0
    };

    let mean_entropy = if non_zero_count > 0 {
        entropy_sum / non_zero_count as f64
    } else {
        0.0
    };

    let high_entropy_pct = if non_zero_count > 0 {
        high_entropy_count as f64 / non_zero_count as f64
    } else {
        0.0
    };

    // Account for zero blocks in compression estimates
    let zero_compressed_approx = zero_count as u64 * 20;
    let lz4_total_with_zeros = lz4_compressed_total + zero_compressed_approx;
    let zstd_total_with_zeros = zstd_compressed_total + zero_compressed_approx;

    let lz4_ratio = if raw_sampled_total > 0 {
        lz4_total_with_zeros as f64 / raw_sampled_total as f64
    } else {
        1.0
    };
    let zstd_ratio = if raw_sampled_total > 0 {
        zstd_total_with_zeros as f64 / raw_sampled_total as f64
    } else {
        1.0
    };

    let lz4_savings_pct = (1.0 - lz4_ratio) * 100.0;
    let zstd_savings_pct = (1.0 - zstd_ratio) * 100.0;
    let estimated_lz4_size = (file_size as f64 * lz4_ratio) as u64;
    let estimated_zstd_size = (file_size as f64 * zstd_ratio) as u64;

    // Fixed dedup ratio
    let fixed_dedup_ratio = if raw_sampled_total > 0 {
        unique_sampled_bytes as f64 / raw_sampled_total as f64
    } else {
        1.0
    };
    let fixed_dedup_savings_pct = (1.0 - fixed_dedup_ratio) * 100.0;

    // Phase 2: CDC analysis — auto-detect params via DCAM if not specified
    f.seek(SeekFrom::Start(0))?;

    // First pass: analyze with LBFS baseline to get change rate
    let baseline = DedupeParams::lbfs_baseline();
    let baseline_stats = analyze_stream(f.try_clone()?, &baseline)?;

    let cdc_params = if let (Some(min), Some(avg), Some(max)) =
        (config.min_chunk, config.avg_chunk, config.max_chunk)
    {
        // User provided all three — use them directly
        let f_bits = (avg as f64).log2().round() as u32;
        DedupeParams {
            f: f_bits,
            m: min,
            z: max,
            w: 48,
            v: 52,
        }
    } else {
        // DCAM auto-detection
        let optimized = optimize_params(file_size, baseline_stats.unique_bytes, &baseline, false);
        let mut params = optimized.params;
        if let Some(min) = config.min_chunk {
            params.m = min;
        }
        if let Some(avg) = config.avg_chunk {
            params.f = (avg as f64).log2().round() as u32;
        }
        if let Some(max) = config.max_chunk {
            params.z = max;
        }
        params
    };

    // Second pass: analyze with resolved params
    let scan_limit = config.dedup_scan_limit.min(file_size);
    f.seek(SeekFrom::Start(0))?;
    let reader = f.by_ref().take(scan_limit);

    let cdc_stats = analyze_stream(reader, &cdc_params)?;

    // Direct measured dedup ratio from the scan
    let cdc_dedup_ratio = if cdc_stats.chunk_count > 0 && cdc_stats.unique_chunk_count > 0 {
        cdc_stats.unique_bytes as f64
            / (cdc_stats.unique_bytes as f64 * cdc_stats.chunk_count as f64
                / cdc_stats.unique_chunk_count as f64)
    } else {
        1.0
    };
    let cdc_dedup_savings_pct = (1.0 - cdc_dedup_ratio) * 100.0;

    // Combined estimates
    let estimated_packed_size_lz4_fixed = (file_size as f64 * lz4_ratio * fixed_dedup_ratio) as u64;
    let estimated_packed_size_zstd_cdc = (file_size as f64 * zstd_ratio * cdc_dedup_ratio) as u64;

    let best_packed = estimated_packed_size_lz4_fixed.min(estimated_packed_size_zstd_cdc);
    let overall_best_savings_pct = (1.0 - best_packed as f64 / file_size as f64) * 100.0;

    Ok(PredictReport {
        file_path: config.path.display().to_string(),
        file_size,
        block_size: config.block_size,
        blocks_sampled,

        zero_block_pct,
        mean_entropy,
        high_entropy_pct,

        lz4_ratio,
        lz4_savings_pct,
        zstd_ratio,
        zstd_savings_pct,
        estimated_lz4_size,
        estimated_zstd_size,

        fixed_dedup_ratio,
        fixed_dedup_savings_pct,

        cdc_scan_bytes: scan_limit,
        cdc_min_chunk: cdc_params.m,
        cdc_avg_chunk: 1 << cdc_params.f,
        cdc_max_chunk: cdc_params.z,
        cdc_chunks_total: cdc_stats.chunk_count,
        cdc_chunks_unique: cdc_stats.unique_chunk_count,
        cdc_dedup_ratio,
        cdc_dedup_savings_pct,

        estimated_packed_size_lz4_fixed,
        estimated_packed_size_zstd_cdc,
        overall_best_savings_pct,
    })
}
