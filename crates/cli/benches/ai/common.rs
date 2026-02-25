//! Shared utilities for AI/ML benchmarks.
//!
//! Provides helper functions for creating synthetic datasets that can be
//! used across all AI benchmark modules for consistency.

use hexz_cli::cmd::data::pack;
use hexz_core::File;
use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_store::local::FileBackend;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use tempfile::NamedTempFile;

/// Creates a synthetic dataset with deterministic, compressible data.
///
/// Generates a Hexz snapshot file containing `num_samples` samples,
/// each of size `sample_size` bytes. The data is deterministic (based
/// on sample index) for reproducibility across benchmark runs.
///
/// Returns a tuple of (input_file, output_file, Arc<File>) where the
/// files must be kept alive for the duration of the benchmark.
pub fn create_dataset(
    num_samples: usize,
    sample_size: usize,
) -> (NamedTempFile, NamedTempFile, Arc<File>) {
    let input = NamedTempFile::new().unwrap();
    let output = NamedTempFile::new().unwrap();

    // Generate deterministic sample data
    for i in 0..num_samples {
        let sample = vec![(i % 256) as u8; sample_size];
        input.as_file().write_all(&sample).unwrap();
    }

    // Build Hexz snapshot with LZ4 compression
    pack::run(
        Some(input.path().to_path_buf()),
        None,
        output.path().to_path_buf(),
        "lz4".to_string(),
        false, // no encryption
        false, // no dict training
        sample_size as u32,
        false, // no CDC
        16384, // min_chunk
        sample_size as u32,
        (sample_size * 2) as u32,
        true,
    )
    .unwrap();

    let snapshot = open_snapshot(output.path());
    (input, output, snapshot)
}

/// Opens a File from a path with default settings.
///
/// Uses FileBackend for local files and Lz4Compressor for decompression.
pub fn open_snapshot(path: &Path) -> Arc<File> {
    let backend = Arc::new(FileBackend::new(path).unwrap());
    let compressor = Box::new(Lz4Compressor::new());
    File::new(backend, compressor, None).unwrap()
}
