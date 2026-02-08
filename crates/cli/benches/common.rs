//! Shared utilities for CLI benchmarks.
//!
//! Provides deterministic, compressible data generation and temporary file
//! helpers used by multiple benchmark suites (concurrency, throughput,
//! gzip comparison, sparse access) so that workloads are consistent.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::fs::File;
use std::io::{BufWriter, Write};

/// Synthesizes a deterministic, moderately compressible data block for benchmarks.
///
/// **Architectural intent:** Provides a reproducible 1 MiB byte pattern with limited
/// entropy so that compression backends and storage layouts can be compared under a
/// stable, compressible workload.
///
/// **Constraints:** The RNG is seeded with a fixed value to guarantee identical output
/// across runs; changing the seed or character set will invalidate comparisons between
/// historical benchmark results.
///
/// **Side effects:** Allocates a contiguous in-memory buffer of approximately 1 MiB and
/// performs pseudo-random generation work proportional to `chunk_size`.
pub fn generate_compressible_chunk() -> Vec<u8> {
    let mut rng = StdRng::seed_from_u64(42);
    let charset = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789    ";
    let chunk_size = 1024 * 1024;
    let mut data = Vec::with_capacity(chunk_size);
    for _ in 0..chunk_size {
        let idx = rng.gen_range(0..charset.len());
        data.push(charset[idx]);
    }
    data
}

/// Writes a large, compressible test file by repeating a pre-generated chunk.
///
/// **Architectural intent:** Efficiently materializes on-disk workloads of arbitrary
/// size while keeping generation cost low, enabling throughput and latency benchmarks
/// that approximate real-world sequential data without external fixtures.
///
/// **Constraints:** The caller must provide an open `File` with sufficient capacity
/// for `total_size` bytes; short writes or underlying filesystem errors will cause the
/// function to panic due to the use of `unwrap()`. The same chunk contents are reused,
/// so the effective entropy profile is periodic rather than fully random.
///
/// **Side effects:** Performs buffered, blocking writes to the provided file handle and
/// flushes the operating-system write buffers before returning.
pub fn write_large_file(file: &File, total_size: usize) {
    let chunk = generate_compressible_chunk();
    let mut writer = BufWriter::new(file);
    let mut written = 0;

    while written < total_size {
        let remaining = total_size - written;
        let to_write = std::cmp::min(remaining, chunk.len());
        writer.write_all(&chunk[..to_write]).unwrap();
        written += to_write;
    }
    writer.flush().unwrap();
}
