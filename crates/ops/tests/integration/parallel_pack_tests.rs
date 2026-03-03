//! Integration tests for parallel packing pipeline.
//!
//! These tests verify that the parallel compression pipeline produces
//! correct output and works with various configurations.

use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_ops::pack::PackConfig;
use hexz_ops::parallel_pack::{
    CompressedChunk, ParallelPackConfig, RawChunk, process_chunks_parallel,
};

/// Test that parallel processing produces correct compressed output.
#[test]
fn test_parallel_chunks_correctness() {
    let chunks: Vec<_> = (0..50)
        .map(|i| RawChunk {
            data: vec![i as u8; 4096],
            logical_offset: i * 4096,
        })
        .collect();

    let compressor = Box::new(Lz4Compressor::new());
    let config = ParallelPackConfig {
        num_workers: 4,
        max_chunks_in_flight: 20,
    };

    let mut results: Vec<(u64, usize, [u8; 32])> = Vec::new();
    let result = process_chunks_parallel(chunks, compressor, config, |chunk: CompressedChunk| {
        results.push((chunk.logical_offset, chunk.original_size, chunk.hash));
        Ok(())
    });

    assert!(result.is_ok());
    assert_eq!(results.len(), 50);

    // All chunks should have original_size == 4096
    for (_, size, _) in &results {
        assert_eq!(*size, 4096);
    }

    // Hashes should be unique for unique inputs
    let mut hashes: Vec<[u8; 32]> = results.iter().map(|(_, _, h)| *h).collect();
    hashes.sort();
    hashes.dedup();
    assert_eq!(
        hashes.len(),
        50,
        "All 50 different chunks should have unique hashes"
    );
}

/// Test parallel processing with duplicate data.
#[test]
fn test_parallel_chunks_with_duplicates() {
    // Create chunks where every other one is identical
    let chunks: Vec<_> = (0..20)
        .map(|i| RawChunk {
            data: vec![(i % 2) as u8; 4096],
            logical_offset: i * 4096,
        })
        .collect();

    let compressor = Box::new(Lz4Compressor::new());
    let config = ParallelPackConfig {
        num_workers: 2,
        max_chunks_in_flight: 10,
    };

    let mut hashes = Vec::new();
    let result = process_chunks_parallel(chunks, compressor, config, |chunk: CompressedChunk| {
        hashes.push(chunk.hash);
        Ok(())
    });

    assert!(result.is_ok());
    assert_eq!(hashes.len(), 20);

    // Should only have 2 unique hashes
    let mut unique_hashes = hashes.clone();
    unique_hashes.sort();
    unique_hashes.dedup();
    assert_eq!(
        unique_hashes.len(),
        2,
        "Should have exactly 2 unique hashes for alternating data"
    );
}

/// Test parallel processing with a single worker (degenerates to serial).
#[test]
fn test_parallel_single_worker() {
    let chunks: Vec<_> = (0..10)
        .map(|i| RawChunk {
            data: vec![i as u8; 1024],
            logical_offset: i * 1024,
        })
        .collect();

    let compressor = Box::new(Lz4Compressor::new());
    let config = ParallelPackConfig {
        num_workers: 1,
        max_chunks_in_flight: 5,
    };

    let mut count = 0;
    let result = process_chunks_parallel(chunks, compressor, config, |_chunk: CompressedChunk| {
        count += 1;
        Ok(())
    });

    assert!(result.is_ok());
    assert_eq!(count, 10);
}

/// Test parallel processing with large number of workers.
#[test]
fn test_parallel_many_workers() {
    let chunks: Vec<_> = (0..5)
        .map(|i| RawChunk {
            data: vec![i as u8; 512],
            logical_offset: i * 512,
        })
        .collect();

    let compressor = Box::new(Lz4Compressor::new());
    let config = ParallelPackConfig {
        num_workers: 16, // More workers than chunks
        max_chunks_in_flight: 100,
    };

    let mut count = 0;
    let result = process_chunks_parallel(chunks, compressor, config, |_chunk: CompressedChunk| {
        count += 1;
        Ok(())
    });

    assert!(result.is_ok());
    assert_eq!(count, 5);
}

/// Test that writer errors propagate correctly.
#[test]
fn test_parallel_writer_error_propagation() {
    let chunks: Vec<_> = (0..10)
        .map(|i| RawChunk {
            data: vec![i as u8; 1024],
            logical_offset: i * 1024,
        })
        .collect();

    let compressor = Box::new(Lz4Compressor::new());
    let config = ParallelPackConfig {
        num_workers: 2,
        max_chunks_in_flight: 5,
    };

    let mut count = 0;
    let result = process_chunks_parallel(chunks, compressor, config, |_chunk: CompressedChunk| {
        count += 1;
        if count >= 3 {
            Err(hexz_common::Error::Io(std::io::Error::other(
                "simulated write error",
            )))
        } else {
            Ok(())
        }
    });

    assert!(result.is_err());
}

/// Test that the compressed data is actually valid LZ4.
#[test]
fn test_parallel_compression_validity() {
    let original_data = vec![0x42u8; 8192];
    let chunks = vec![RawChunk {
        data: original_data.clone(),
        logical_offset: 0,
    }];

    let compressor = Box::new(Lz4Compressor::new());
    let config = ParallelPackConfig {
        num_workers: 2,
        max_chunks_in_flight: 10,
    };

    let mut compressed_data = None;
    let result = process_chunks_parallel(chunks, compressor, config, |chunk: CompressedChunk| {
        compressed_data = Some(chunk.compressed);
        Ok(())
    });

    assert!(result.is_ok());
    let compressed = compressed_data.expect("Should have received compressed data");

    // Verify the compressed data can be decompressed back
    let decompressor = Lz4Compressor::new();
    use hexz_core::algo::compression::Compressor;
    let decompressed = decompressor.decompress(&compressed).unwrap();
    assert_eq!(decompressed, original_data);
}

/// Test PackConfig parallel defaults are set correctly.
#[test]
fn test_pack_config_parallel_defaults() {
    let config = PackConfig::default();
    assert!(config.parallel, "parallel should default to true");
    assert_eq!(
        config.num_workers, 0,
        "num_workers should default to 0 (auto-detect)"
    );
    assert!(config.show_progress, "show_progress should default to true");
}

/// Test PackConfig can be constructed with parallel settings.
#[test]
fn test_pack_config_with_parallel_settings() {
    let config = PackConfig {
        input: std::path::PathBuf::from("test.img"),
        output: std::path::PathBuf::from("test.hxz"),
        parallel: false,
        num_workers: 4,
        show_progress: false,
        ..Default::default()
    };

    assert!(!config.parallel);
    assert_eq!(config.num_workers, 4);
    assert!(!config.show_progress);
}
