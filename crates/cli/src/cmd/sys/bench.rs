//! Implementation of the `strata bench` command.
//!
//! Runs read throughput benchmarks against a Strata snapshot to measure decompression
//! and I/O performance. This helps validate storage backend configuration and identify
//! bottlenecks in snapshot access patterns.
//!
//! # Benchmarks
//!
//! The benchmark suite currently includes:
//!
//! - **Sequential read test**: Reads the entire disk stream sequentially in 1 MiB chunks
//!   and measures total throughput (MB/s). This tests decompression speed and storage
//!   backend bandwidth.
//!
//! Future benchmarks may include:
//! - Random IOPS testing (4 KiB random reads)
//! - Cache effectiveness measurements
//! - Multi-threaded access patterns
//!
//! # Performance Expectations
//!
//! Typical sequential read throughput:
//! - **LZ4 compression**: 800-2000 MB/s (decompression-bound)
//! - **Zstd compression**: 400-800 MB/s (decompression-bound)
//! - **Local SSD backend**: Usually CPU-bound (decompression bottleneck)
//! - **Remote backends (S3, HTTP)**: May be network-bound at 100-500 MB/s
//!
//! # Output
//!
//! The benchmark displays:
//! - Snapshot size (logical disk size)
//! - Progress bar during the test
//! - Total bytes read
//! - Test duration
//! - Average throughput (MB/s)

use anyhow::Result;
use indicatif::{HumanBytes, ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use strata_common::constants::DEFAULT_ZSTD_LEVEL;
use strata_core::StrataFile;
use strata_core::algo::compression::{Compressor, lz4::Lz4Compressor, zstd::ZstdCompressor};
use strata_core::api::stratafile::SnapshotStream;
use strata_core::format::header::CompressionType;
use strata_core::format::magic::HEADER_SIZE;
use strata_core::store::StorageBackend;
use strata_core::store::local::FileBackend;

/// Execute the benchmark command on a Strata snapshot.
///
/// This function runs a sequential read benchmark to measure snapshot read performance.
/// It opens the snapshot, configures the appropriate decompressor, and reads the entire
/// disk stream in 1 MiB chunks while measuring throughput.
///
/// # Benchmark Methodology
///
/// The test performs a single sequential pass over the entire disk stream:
/// 1. Opens the snapshot and loads the header
/// 2. Configures the decompressor (LZ4 or Zstd) with any embedded dictionary
/// 3. Reads the disk stream sequentially in 1 MiB chunks
/// 4. Measures total time and calculates throughput
///
/// # Arguments
///
/// * `snap_path` - Path to the `.st` snapshot file
/// * `_block_size` - Reserved for future random I/O tests (currently unused)
/// * `_duration` - Reserved for time-limited tests (currently unused)
/// * `_threads` - Reserved for multi-threaded tests (currently unused)
///
/// # Performance Notes
///
/// This benchmark tests the full read path including:
/// - Storage backend I/O (file reads, network fetches)
/// - Decompression (LZ4 or Zstd)
/// - Block cache effectiveness (if cache is enabled)
///
/// For accurate results:
/// - Run on a representative hardware configuration
/// - Ensure the snapshot is not cached in system page cache (or run multiple iterations)
/// - Compare results across compression algorithms and block sizes
///
/// # Example Output
///
/// ```text
/// Benchmarking snapshot: "vm-snapshot.st"
/// Image Size: 10.0 GB
///
/// Running Sequential Read Test (1 pass)...
/// [████████████████████] 10.0 GB/10.0 GB (250 MB/s)
///
/// Total Read: 10.0 GB
/// Duration: 40.23s
/// Throughput: 248.57 MB/s
/// ```
pub fn run(
    snap_path: PathBuf,
    _block_size: Option<u32>,
    _duration: Option<u64>,
    _threads: Option<usize>,
) -> Result<()> {
    println!("Benchmarking snapshot: {:?}", snap_path);
    let backend = Arc::new(FileBackend::new(&snap_path)?);

    let header_bytes = backend.read_exact(0, HEADER_SIZE)?;
    let header: strata_core::format::header::StrataHeader = bincode::deserialize(&header_bytes)?;

    let dictionary = if let (Some(offset), Some(length)) =
        (header.dictionary_offset, header.dictionary_length)
    {
        Some(backend.read_exact(offset, length as usize)?.to_vec())
    } else {
        None
    };

    let compressor: Box<dyn Compressor> = match header.compression {
        CompressionType::Lz4 => Box::new(Lz4Compressor::new()),
        CompressionType::Zstd => Box::new(ZstdCompressor::new(DEFAULT_ZSTD_LEVEL, dictionary)),
    };

    let snap = StrataFile::new(backend, compressor, None)?;
    let disk_size = snap.size(SnapshotStream::Disk);

    println!("Image Size: {}", HumanBytes(disk_size));

    // Sequential Read Test
    println!("\nRunning Sequential Read Test (1 pass)...");
    let start = Instant::now();
    let mut offset = 0;
    let chunk_size = 1024 * 1024; // 1 MiB chunks
    let mut total_read = 0;
    let pb = ProgressBar::new(disk_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec})")
        .unwrap());

    while offset < disk_size {
        let len = std::cmp::min(chunk_size, (disk_size - offset) as usize);
        let _data = snap.read_at(SnapshotStream::Disk, offset, len)?;
        offset += len as u64;
        total_read += len as u64;
        pb.inc(len as u64);
    }
    pb.finish_with_message("Done");

    let duration = start.elapsed();
    let mb = total_read as f64 / 1024.0 / 1024.0;
    let throughput = mb / duration.as_secs_f64();

    println!("Total Read: {}", HumanBytes(total_read));
    println!("Duration: {:.2?}", duration);
    println!("Throughput: {:.2} MB/s", throughput);

    // Random Read Test (if requested or default)
    // Note: Implementing true random IOPS benchmark requires threads and duration control.
    // For now, we provide a basic sequential bench.

    Ok(())
}
