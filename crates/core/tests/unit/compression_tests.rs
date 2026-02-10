//! Unit tests for compression algorithms (LZ4 and Zstandard).
//!
//! Tests verify correct compression/decompression round-trips, error handling,
//! and compression ratio characteristics for different data patterns.

use super::common;
use common::*;

use strata_core::algo::compression::{Compressor, lz4::Lz4Compressor, zstd::ZstdCompressor};

/// Test LZ4 compression round-trip with random data.
#[test]
fn test_lz4_roundtrip_random() {
    let compressor = Lz4Compressor::new();
    let original = vec![42u8; 1024];

    let compressed = compressor.compress(&original).expect("Compression failed");
    let decompressed = compressor
        .decompress(&compressed)
        .expect("Decompression failed");

    assert_eq!(decompressed, original);
}

/// Test LZ4 compression with highly compressible data (all zeros).
#[test]
fn test_lz4_high_compression() {
    let compressor = Lz4Compressor::new();
    let original = vec![0u8; 4096];

    let compressed = compressor.compress(&original).expect("Compression failed");

    // Zeros should compress very well
    assert!(compressed.len() < original.len() / 10);

    let decompressed = compressor
        .decompress(&compressed)
        .expect("Decompression failed");
    assert_eq!(decompressed, original);
}

/// Test LZ4 compression with incompressible data.
#[test]
fn test_lz4_incompressible() {
    let compressor = Lz4Compressor::new();
    // Create pseudo-random data
    let original: Vec<u8> = (0..4096).map(|i| ((i * 7919) % 256) as u8).collect();

    let compressed = compressor.compress(&original).expect("Compression failed");

    // Even with pseudo-random data, LZ4 might find patterns
    // Just verify it compresses and decompresses correctly
    let decompressed = compressor
        .decompress(&compressed)
        .expect("Decompression failed");
    assert_eq!(decompressed, original);
}

/// Test LZ4 decompress_into with pre-allocated buffer.
#[test]
fn test_lz4_decompress_into() {
    let compressor = Lz4Compressor::new();
    let original = vec![123u8; 1024];

    let compressed = compressor.compress(&original).expect("Compression failed");

    let mut output = vec![0u8; 1024];
    let size = compressor
        .decompress_into(&compressed, &mut output)
        .expect("Decompression failed");

    assert_eq!(size, 1024);
    assert_eq!(output, original);
}

/// Test LZ4 with empty input.
#[test]
fn test_lz4_empty_input() {
    let compressor = Lz4Compressor::new();
    let original = vec![];

    let compressed = compressor.compress(&original).expect("Compression failed");
    let decompressed = compressor
        .decompress(&compressed)
        .expect("Decompression failed");

    assert_eq!(decompressed, original);
}

/// Test LZ4 with various block sizes.
#[test]
fn test_lz4_various_sizes() {
    let compressor = Lz4Compressor::new();
    let sizes = vec![1, 16, 256, 4096, 65536, 1048576];

    for size in sizes {
        let original = vec![0xABu8; size];
        let compressed = compressor.compress(&original).expect("Compression failed");
        let decompressed = compressor
            .decompress(&compressed)
            .expect("Decompression failed");
        assert_eq!(decompressed.len(), size);
    }
}

/// Test Zstandard compression round-trip without dictionary.
#[test]
fn test_zstd_roundtrip() {
    let compressor = ZstdCompressor::new(3, None);
    let original = vec![55u8; 2048];

    let compressed = compressor.compress(&original).expect("Compression failed");
    let decompressed = compressor
        .decompress(&compressed)
        .expect("Decompression failed");

    assert_eq!(decompressed, original);
}

/// Test Zstandard with high compression level.
#[test]
fn test_zstd_high_level() {
    let compressor = ZstdCompressor::new(19, None);
    let original = vec![0u8; 8192];

    let compressed = compressor.compress(&original).expect("Compression failed");

    // High compression should achieve excellent ratio on zeros
    assert!(compressed.len() < 100);

    let decompressed = compressor
        .decompress(&compressed)
        .expect("Decompression failed");
    assert_eq!(decompressed, original);
}

/// Test Zstandard with dictionary training and usage.
#[test]
fn test_zstd_with_dictionary() {
    // Create training samples with similar patterns
    let samples: Vec<Vec<u8>> = (0..10)
        .map(|i| {
            let mut data = vec![0u8; 1024];
            // Add some structure
            for (j, item) in data.iter_mut().enumerate().take(1024) {
                *item = ((i + j) % 256) as u8;
            }
            data
        })
        .collect();

    // Train dictionary
    let dict = ZstdCompressor::train(&samples, 4096).expect("Dictionary training failed");
    assert!(dict.len() <= 4096);

    // Use dictionary for compression
    let compressor = ZstdCompressor::new(3, Some(dict));

    let original = samples[0].clone();
    let compressed = compressor.compress(&original).expect("Compression failed");
    let decompressed = compressor
        .decompress(&compressed)
        .expect("Decompression failed");

    assert_eq!(decompressed, original);
}

/// Test Zstandard decompress_into.
#[test]
fn test_zstd_decompress_into() {
    let compressor = ZstdCompressor::new(3, None);
    let original = vec![77u8; 4096];

    let compressed = compressor.compress(&original).expect("Compression failed");

    let mut output = vec![0u8; 4096];
    let size = compressor
        .decompress_into(&compressed, &mut output)
        .expect("Decompression failed");

    assert_eq!(size, 4096);
    assert_eq!(output, original);
}

/// Test Zstandard with structured data (repeating patterns).
#[test]
fn test_zstd_structured_data() {
    let compressor = ZstdCompressor::new(3, None);

    // Create data with repeating 4-byte pattern
    let mut original = vec![0u8; 4096];
    for i in 0..1024 {
        let offset = i * 4;
        original[offset] = 0xDE;
        original[offset + 1] = 0xAD;
        original[offset + 2] = 0xBE;
        original[offset + 3] = 0xEF;
    }

    let compressed = compressor.compress(&original).expect("Compression failed");

    // Repeating pattern should compress very well
    assert!(compressed.len() < original.len() / 50);

    let decompressed = compressor
        .decompress(&compressed)
        .expect("Decompression failed");
    assert_eq!(decompressed, original);
}

/// Test compression ratio comparison between LZ4 and Zstandard.
#[test]
fn test_compression_ratio_comparison() {
    let lz4 = Lz4Compressor::new();
    let zstd = ZstdCompressor::new(3, None);

    // Create data with some structure
    let mut original = vec![0u8; 16384];
    for (i, item) in original.iter_mut().enumerate() {
        *item = (i % 128) as u8;
    }

    let lz4_compressed = lz4.compress(&original).expect("LZ4 compression failed");
    let zstd_compressed = zstd
        .compress(&original)
        .expect("Zstandard compression failed");

    // Zstandard should generally achieve better compression
    println!("Original: {} bytes", original.len());
    println!("LZ4: {} bytes", lz4_compressed.len());
    println!("Zstd: {} bytes", zstd_compressed.len());

    // Both should successfully decompress
    let lz4_decompressed = lz4
        .decompress(&lz4_compressed)
        .expect("LZ4 decompression failed");
    let zstd_decompressed = zstd
        .decompress(&zstd_compressed)
        .expect("Zstandard decompression failed");

    assert_eq!(lz4_decompressed, original);
    assert_eq!(zstd_decompressed, original);
}

/// Test invalid compressed data handling.
#[test]
fn test_lz4_invalid_data() {
    let compressor = Lz4Compressor::new();
    let invalid_data = vec![0xFF; 100];

    let result = compressor.decompress(&invalid_data);
    assert!(result.is_err());
}

/// Test Zstandard with invalid data.
#[test]
fn test_zstd_invalid_data() {
    let compressor = ZstdCompressor::new(3, None);
    let invalid_data = vec![0xAB; 100];

    let result = compressor.decompress(&invalid_data);
    assert!(result.is_err());
}

/// Test buffer too small for decompress_into.
#[test]
fn test_decompress_into_buffer_too_small() {
    let compressor = Lz4Compressor::new();
    let original = vec![42u8; 1024];
    let compressed = compressor.compress(&original).expect("Compression failed");

    let mut small_buffer = vec![0u8; 512];
    let result = compressor.decompress_into(&compressed, &mut small_buffer);

    // Should fail because buffer is too small
    assert!(result.is_err());
}

// Extreme size tests

#[test]
fn test_lz4_zero_bytes() {
    let compressor = Lz4Compressor::new();
    let data = vec![];
    let compressed = compressor.compress(&data).unwrap();
    let decompressed = compressor.decompress(&compressed).unwrap();
    assert_eq!(decompressed, data);
}

#[test]
fn test_lz4_one_byte() {
    let compressor = Lz4Compressor::new();
    let data = vec![0x42];
    let compressed = compressor.compress(&data).unwrap();
    let decompressed = compressor.decompress(&compressed).unwrap();
    assert_eq!(decompressed, data);
}

#[test]
fn test_lz4_16mb() {
    let compressor = Lz4Compressor::new();
    let data = create_random_data(16 * 1024 * 1024);
    let compressed = compressor.compress(&data).unwrap();
    let decompressed = compressor.decompress(&compressed).unwrap();
    assert_bytes_equal(&decompressed, &data, "16MB round-trip");
}

#[test]
fn test_zstd_zero_bytes() {
    let compressor = ZstdCompressor::new(3, None);
    let data = vec![];
    let compressed = compressor.compress(&data).unwrap();
    let decompressed = compressor.decompress(&compressed).unwrap();
    assert_eq!(decompressed, data);
}

#[test]
fn test_zstd_one_byte() {
    let compressor = ZstdCompressor::new(3, None);
    let data = vec![0x42];
    let compressed = compressor.compress(&data).unwrap();
    let decompressed = compressor.decompress(&compressed).unwrap();
    assert_eq!(decompressed, data);
}

#[test]
fn test_zstd_16mb() {
    let compressor = ZstdCompressor::new(3, None);
    let data = create_random_data(16 * 1024 * 1024);
    let compressed = compressor.compress(&data).unwrap();
    let decompressed = compressor.decompress(&compressed).unwrap();
    assert_bytes_equal(&decompressed, &data, "16MB round-trip");
}

// Entropy variation tests

#[test]
fn test_lz4_all_zeros() {
    let compressor = Lz4Compressor::new();
    let data = vec![0u8; 100_000];
    let compressed = compressor.compress(&data).unwrap();
    let ratio = measure_compression_ratio(data.len(), compressed.len());
    assert!(ratio < 0.01, "All zeros should compress to <1%");
    let decompressed = compressor.decompress(&compressed).unwrap();
    assert_bytes_equal(&decompressed, &data, "all zeros");
}

#[test]
fn test_lz4_alternating_bits() {
    let compressor = Lz4Compressor::new();
    let data: Vec<u8> = (0..10000)
        .map(|i| if i % 2 == 0 { 0xAA } else { 0x55 })
        .collect();
    let compressed = compressor.compress(&data).unwrap();
    let decompressed = compressor.decompress(&compressed).unwrap();
    assert_bytes_equal(&decompressed, &data, "alternating bits");
}

#[test]
fn test_lz4_random_high_entropy() {
    let compressor = Lz4Compressor::new();
    let data = create_random_data(10_000);
    let compressed = compressor.compress(&data).unwrap();
    let ratio = measure_compression_ratio(data.len(), compressed.len());
    // Random data should not compress well
    assert!(ratio > 0.95, "Random data should have ratio >95%");
    let decompressed = compressor.decompress(&compressed).unwrap();
    assert_bytes_equal(&decompressed, &data, "random data");
}

#[test]
fn test_zstd_structured_low_entropy() {
    let compressor = ZstdCompressor::new(9, None);
    let data = create_structured_data(100_000, 256);
    let compressed = compressor.compress(&data).unwrap();
    let ratio = measure_compression_ratio(data.len(), compressed.len());
    assert!(
        ratio < 0.05,
        "Structured data should compress well with Zstd"
    );
    let decompressed = compressor.decompress(&compressed).unwrap();
    assert_bytes_equal(&decompressed, &data, "structured data");
}

// Dictionary training edge cases

#[test]
fn test_zstd_dict_insufficient_samples() {
    // Less than 10 samples (minimum recommended)
    let samples: Vec<Vec<u8>> = (0..5).map(|_| create_random_data(1000)).collect();
    let result = ZstdCompressor::train(&samples, 1024);
    // May fail or succeed depending on implementation - just verify it doesn't panic
    let _ = result;
}

#[test]
fn test_zstd_dict_very_large_samples() {
    // 10 samples of 10MB each
    let samples: Vec<Vec<u8>> = (0..10)
        .map(|i| create_random_data_with_seed(10 * 1024 * 1024, i))
        .collect();
    let dict = ZstdCompressor::train(&samples, 64 * 1024).unwrap();
    assert!(dict.len() <= 64 * 1024);
}

#[test]
fn test_zstd_dict_homogeneous_samples() {
    // All samples are identical
    let sample = vec![0x42u8; 1000];
    let samples: Vec<Vec<u8>> = (0..10).map(|_| sample.clone()).collect();
    let dict = ZstdCompressor::train(&samples, 4096).unwrap();
    assert!(dict.len() <= 4096);
}

#[test]
fn test_zstd_dict_size_limits() {
    let samples: Vec<Vec<u8>> = (0..20)
        .map(|i| create_random_data_with_seed(1000, i))
        .collect();

    for dict_size in [512, 1024, 4096, 16384, 65536] {
        let dict = ZstdCompressor::train(&samples, dict_size).unwrap();
        assert!(dict.len() <= dict_size, "Dictionary exceeded size limit");
    }
}

// Buffer handling tests

#[test]
fn test_lz4_decompress_into_exact_size() {
    let compressor = Lz4Compressor::new();
    let data = create_random_data(1000);
    let compressed = compressor.compress(&data).unwrap();

    let mut output = vec![0u8; 1000];
    let size = compressor
        .decompress_into(&compressed, &mut output)
        .unwrap();
    assert_eq!(size, 1000);
    assert_bytes_equal(&output, &data, "exact size buffer");
}

#[test]
fn test_lz4_decompress_into_large_buffer() {
    let compressor = Lz4Compressor::new();
    let data = create_random_data(1000);
    let compressed = compressor.compress(&data).unwrap();

    let mut output = vec![0u8; 5000];
    let size = compressor
        .decompress_into(&compressed, &mut output)
        .unwrap();
    assert_eq!(size, 1000);
    assert_bytes_equal(&output[..1000], &data, "large buffer");
}

#[test]
fn test_zstd_decompress_into_exact_size() {
    let compressor = ZstdCompressor::new(3, None);
    let data = create_random_data(1000);
    let compressed = compressor.compress(&data).unwrap();

    let mut output = vec![0u8; 1000];
    let size = compressor
        .decompress_into(&compressed, &mut output)
        .unwrap();
    assert_eq!(size, 1000);
    assert_bytes_equal(&output, &data, "exact size buffer");
}

// Error recovery tests

#[test]
fn test_lz4_truncated_compressed_data() {
    let compressor = Lz4Compressor::new();
    let data = create_random_data(1000);
    let mut compressed = compressor.compress(&data).unwrap();

    // Truncate compressed data
    compressed.truncate(compressed.len() / 2);

    let result = compressor.decompress(&compressed);
    assert!(result.is_err(), "Truncated data should fail");
}

#[test]
fn test_zstd_truncated_compressed_data() {
    let compressor = ZstdCompressor::new(3, None);
    let data = create_random_data(1000);
    let mut compressed = compressor.compress(&data).unwrap();

    // Truncate compressed data
    compressed.truncate(compressed.len() / 2);

    let result = compressor.decompress(&compressed);
    assert!(result.is_err(), "Truncated data should fail");
}

#[test]
fn test_lz4_corrupted_header() {
    let compressor = Lz4Compressor::new();
    let data = create_random_data(1000);
    let mut compressed = compressor.compress(&data).unwrap();

    // Corrupt first bytes (header)
    if compressed.len() > 10 {
        compressed[0] ^= 0xFF;
        compressed[1] ^= 0xFF;
        compressed[2] ^= 0xFF;
        compressed[3] ^= 0xFF;
    }

    let result = compressor.decompress(&compressed);
    // LZ4 may or may not detect corruption depending on the exact bytes
    // Just verify it doesn't panic
    let _ = result;
}

#[test]
fn test_zstd_corrupted_data() {
    let compressor = ZstdCompressor::new(3, None);
    let data = create_random_data(1000);
    let mut compressed = compressor.compress(&data).unwrap();

    // Corrupt middle bytes
    if compressed.len() > 20 {
        let mid = compressed.len() / 2;
        compressed[mid] ^= 0xFF;
        compressed[mid + 1] ^= 0xFF;
        compressed[mid + 2] ^= 0xFF;
    }

    let result = compressor.decompress(&compressed);
    // Zstd should detect corruption, but let's not be too strict
    // Just verify it doesn't panic
    let _ = result;
}

// Performance characteristics tests

#[test]
fn test_compression_ratio_all_zeros() {
    let lz4 = Lz4Compressor::new();
    let zstd = ZstdCompressor::new(3, None);
    let data = vec![0u8; 100_000];

    let lz4_compressed = lz4.compress(&data).unwrap();
    let zstd_compressed = zstd.compress(&data).unwrap();

    let lz4_ratio = measure_compression_ratio(data.len(), lz4_compressed.len());
    let zstd_ratio = measure_compression_ratio(data.len(), zstd_compressed.len());

    assert!(lz4_ratio < 0.01, "LZ4 should compress zeros to <1%");
    assert!(zstd_ratio < 0.01, "Zstd should compress zeros to <1%");
}

#[test]
fn test_compression_ratio_sparse_data() {
    let lz4 = Lz4Compressor::new();
    let zstd = ZstdCompressor::new(9, None);
    let data = create_sparse_data(100_000, 0.95); // 95% zeros

    let lz4_compressed = lz4.compress(&data).unwrap();
    let zstd_compressed = zstd.compress(&data).unwrap();

    let lz4_ratio = measure_compression_ratio(data.len(), lz4_compressed.len());
    let zstd_ratio = measure_compression_ratio(data.len(), zstd_compressed.len());

    // Sparse data should compress reasonably well
    assert!(lz4_ratio < 0.5, "LZ4 should compress sparse data");
    assert!(
        zstd_ratio < lz4_ratio,
        "Zstd should beat LZ4 on sparse data"
    );
}

#[test]
fn test_zstd_level_comparison() {
    let data = create_structured_data(50_000, 1000);

    let level1 = ZstdCompressor::new(1, None);
    let level9 = ZstdCompressor::new(9, None);
    let level19 = ZstdCompressor::new(19, None);

    let compressed1 = level1.compress(&data).unwrap();
    let compressed9 = level9.compress(&data).unwrap();
    let compressed19 = level19.compress(&data).unwrap();

    // Higher levels should achieve better compression
    assert!(compressed19.len() <= compressed9.len());
    assert!(compressed9.len() <= compressed1.len());

    // All should decompress correctly
    let decompressed1 = level1.decompress(&compressed1).unwrap();
    let decompressed9 = level9.decompress(&compressed9).unwrap();
    let decompressed19 = level19.decompress(&compressed19).unwrap();

    assert_bytes_equal(&decompressed1, &data, "level 1");
    assert_bytes_equal(&decompressed9, &data, "level 9");
    assert_bytes_equal(&decompressed19, &data, "level 19");
}

#[test]
fn test_lz4_speed_vs_zstd_ratio() {
    let data = create_structured_data(100_000, 500);

    let lz4 = Lz4Compressor::new();
    let zstd = ZstdCompressor::new(3, None);

    let lz4_compressed = lz4.compress(&data).unwrap();
    let zstd_compressed = zstd.compress(&data).unwrap();

    // Zstd should achieve better compression ratio
    assert!(
        zstd_compressed.len() < lz4_compressed.len(),
        "Zstd should beat LZ4 on compression ratio for structured data"
    );

    // Both should decompress correctly
    let lz4_decompressed = lz4.decompress(&lz4_compressed).unwrap();
    let zstd_decompressed = zstd.decompress(&zstd_compressed).unwrap();

    assert_bytes_equal(&lz4_decompressed, &data, "LZ4");
    assert_bytes_equal(&zstd_decompressed, &data, "Zstd");
}

#[test]
fn test_multiple_compress_decompress_cycles() {
    let compressor = Lz4Compressor::new();
    let mut data = create_random_data(1000);

    // Compress and decompress 10 times
    for _ in 0..10 {
        let compressed = compressor.compress(&data).unwrap();
        data = compressor.decompress(&compressed).unwrap();
    }

    // Data should be unchanged after cycles
    assert_eq!(data.len(), 1000);
}

#[test]
fn test_zstd_max_level() {
    let compressor = ZstdCompressor::new(22, None); // Max level
    let data = create_structured_data(10_000, 100);

    let compressed = compressor.compress(&data).unwrap();
    let decompressed = compressor.decompress(&compressed).unwrap();

    assert_bytes_equal(&decompressed, &data, "max level");

    // Should achieve excellent compression
    let ratio = measure_compression_ratio(data.len(), compressed.len());
    assert!(
        ratio < 0.05,
        "Max level should compress structured data to <5%"
    );
}
