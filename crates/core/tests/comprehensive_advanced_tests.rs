#![allow(clippy::unwrap_used, clippy::expect_used, unused_results, clippy::unreadable_literal, clippy::significant_drop_tightening, clippy::needless_pass_by_value, clippy::float_cmp)]
//! Comprehensive Advanced Testing
//!
//! Real, working advanced tests for the Hexz codebase covering:
//! - Property-based testing
//! - Edge case testing
//! - Chaos testing
//! - Concurrent testing

use hexz_core::algo::compression::Compressor;
use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_core::algo::hashing::ContentHasher;
use hexz_core::algo::hashing::blake3::Blake3Hasher;

#[cfg(test)]
mod property_based_tests {
    use super::*;
    use proptest::prelude::*;

    /// Property: Compression roundtrip preserves data
    #[test]
    fn proptest_compression_roundtrip_lz4() {
        proptest!(|(data: Vec<u8>)| {
            let compressor = Lz4Compressor::new();

            // Compress
            let compressed = compressor.compress(&data).unwrap();

            // Decompress
            let decompressed = compressor.decompress(&compressed).unwrap();

            // Property: roundtrip should be identity
            prop_assert_eq!(data, decompressed);
        });
    }

    /// Property: Hash is deterministic
    #[test]
    fn proptest_hash_deterministic() {
        proptest!(|(data: Vec<u8>)| {
            let hasher = Blake3Hasher::new();

            let hash1 = hasher.hash(&data).unwrap();
            let hash2 = hasher.hash(&data).unwrap();

            // Property: same input produces same hash
            prop_assert_eq!(hash1, hash2);
        });
    }

    /// Property: Different data produces different hashes
    #[test]
    fn proptest_hash_collision_resistance() {
        proptest!(|(data1: Vec<u8>, data2: Vec<u8>)| {
            prop_assume!(data1 != data2); // Only test when data is different

            let hasher = Blake3Hasher::new();
            let hash1 = hasher.hash(&data1).unwrap();
            let hash2 = hasher.hash(&data2).unwrap();

            // Property: different inputs should produce different hashes
            prop_assert_ne!(hash1, hash2);
        });
    }

    /// Property: Compression should not expand small data too much
    #[test]
    fn proptest_compression_efficiency() {
        proptest!(|(data in prop::collection::vec(any::<u8>(), 1..1000))| {
            let compressor = Lz4Compressor::new();
            let compressed = compressor.compress(&data).unwrap();

            // Property: LZ4 has overhead, so very small data can expand
            // For data > 100 bytes, expansion should be limited
            if data.len() > 100 {
                prop_assert!(compressed.len() <= data.len() * 2);
            } else {
                // Small data can have more overhead due to framing
                prop_assert!(compressed.len() <= data.len() + 100);
            }
        });
    }

    /// Property: Hash output length is consistent
    #[test]
    fn proptest_hash_output_length() {
        proptest!(|(data: Vec<u8>)| {
            let hasher = Blake3Hasher::new();
            let hash = hasher.hash(&data).unwrap();

            // Property: hash length should always be output_len()
            prop_assert_eq!(hash.len(), hasher.output_len());
        });
    }
}

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    /// Test compression of empty data
    #[test]
    fn edge_case_empty_compression() {
        let compressor = Lz4Compressor::new();
        let empty: Vec<u8> = vec![];

        let compressed = compressor.compress(&empty).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();

        assert_eq!(empty, decompressed);
    }

    /// Test compression of single byte
    #[test]
    fn edge_case_single_byte_compression() {
        let compressor = Lz4Compressor::new();
        let single = vec![42u8];

        let compressed = compressor.compress(&single).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();

        assert_eq!(single, decompressed);
    }

    /// Test compression of highly compressible data (all zeros)
    #[test]
    fn edge_case_all_zeros_compression() {
        let compressor = Lz4Compressor::new();
        let zeros = vec![0u8; 64 * 1024]; // 64KB of zeros

        let compressed = compressor.compress(&zeros).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();

        assert_eq!(zeros, decompressed);

        // Compressed size should be much smaller than original
        assert!(compressed.len() < zeros.len() / 10);
    }

    /// Test compression of highly compressible data (repeated pattern)
    #[test]
    fn edge_case_repeated_pattern_compression() {
        let compressor = Lz4Compressor::new();
        let pattern = b"ABCD".repeat(16 * 1024); // 64KB of "ABCD"

        let compressed = compressor.compress(&pattern).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();

        assert_eq!(pattern, decompressed);

        // Should compress very well
        assert!(compressed.len() < pattern.len() / 10);
    }

    /// Test compression of incompressible data (random-like)
    #[test]
    fn edge_case_random_compression() {
        let compressor = Lz4Compressor::new();
        // Generate pseudo-random data
        let random: Vec<u8> = (0..64 * 1024)
            .map(|i| ((i * 17 + 42) % 256) as u8)
            .collect();

        let compressed = compressor.compress(&random).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();

        assert_eq!(random, decompressed);

        // Random data doesn't compress well, might even expand slightly
        assert!(compressed.len() <= random.len() * 2);
    }

    /// Test large data compression (16MB)
    #[test]
    #[ignore = "slow: 16 MiB compression round-trip"]
    fn edge_case_large_data_compression() {
        let compressor = Lz4Compressor::new();
        let large = vec![0xAAu8; 16 * 1024 * 1024]; // 16MB

        let compressed = compressor.compress(&large).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();

        assert_eq!(large, decompressed);
    }

    /// Test hash of empty data
    #[test]
    fn edge_case_empty_hash() {
        let hasher = Blake3Hasher::new();
        let empty: Vec<u8> = vec![];

        let hash = hasher.hash(&empty).unwrap();

        // Hash of empty should be deterministic
        let hash2 = hasher.hash(&empty).unwrap();
        assert_eq!(hash, hash2);

        // Should not be all zeros
        assert_ne!(hash, vec![0u8; hash.len()]);
    }

    /// Test hash of single byte
    #[test]
    fn edge_case_single_byte_hash() {
        let hasher = Blake3Hasher::new();

        let hash1 = hasher.hash(&[0u8]).unwrap();
        let hash2 = hasher.hash(&[1u8]).unwrap();

        // Single bit difference should produce different hash
        assert_ne!(hash1, hash2);
    }

    /// Test hash of large data
    #[test]
    #[ignore = "slow: large data hash"]
    fn edge_case_large_hash() {
        let hasher = Blake3Hasher::new();
        let large = vec![0xAAu8; 100 * 1024 * 1024]; // 100MB

        let _hash = hasher.hash(&large).unwrap();
        // Should complete without panic
    }

    /// Test hash output length
    #[test]
    fn edge_case_hash_length() {
        let hasher = Blake3Hasher::new();
        let data = vec![42u8; 1024];

        let hash = hasher.hash(&data).unwrap();

        // Should match declared output length
        assert_eq!(hash.len(), hasher.output_len());
        // Blake3 outputs 32 bytes
        assert_eq!(hash.len(), 32);
    }

    /// Test `hash_fixed` returns 32 bytes
    #[test]
    fn edge_case_hash_fixed() {
        let hasher = Blake3Hasher::new();
        let data = vec![42u8; 1024];

        let hash = hasher.hash_fixed(&data);

        // Should always be 32 bytes
        assert_eq!(hash.len(), 32);

        // Should match hash()
        let hash_vec = hasher.hash(&data).unwrap();
        assert_eq!(&hash[..], &hash_vec[..]);
    }
}

#[cfg(test)]
mod chaos_tests {
    use super::*;
    use std::env;

    fn chaos_mode() -> bool {
        env::var("HEXZ_CHAOS_MODE").is_ok()
    }

    /// Test compression with corrupted compressed data
    #[test]
    fn chaos_corrupted_compression() {
        if !chaos_mode() {
            return;
        }

        let compressor = Lz4Compressor::new();
        let data = vec![42u8; 1024];

        let mut compressed = compressor.compress(&data).unwrap();

        // Corrupt random bytes
        if compressed.len() > 10 {
            compressed[5] ^= 0xFF;
            let mid = compressed.len() / 2;
            compressed[mid] ^= 0xFF;

            // Decompression should fail gracefully (not panic)
            let result = compressor.decompress(&compressed);
            assert!(result.is_err(), "Should detect corruption");
        }
    }

    /// Test decompression with truncated data
    #[test]
    fn chaos_truncated_data() {
        if !chaos_mode() {
            return;
        }

        let compressor = Lz4Compressor::new();
        let data = vec![42u8; 1024];

        let compressed = compressor.compress(&data).unwrap();

        // Try various truncations
        if compressed.len() > 10 {
            for size in [1, compressed.len() / 2, compressed.len() - 1] {
                let truncated = &compressed[..size];
                let result = compressor.decompress(truncated);
                // Should fail gracefully
                assert!(result.is_err() || result.unwrap() != data);
            }
        }
    }

    /// Test hash with various edge cases
    #[test]
    fn chaos_hash_edge_cases() {
        if !chaos_mode() {
            return;
        }

        let hasher = Blake3Hasher::new();

        // Test various patterns that might expose bugs
        let test_cases = vec![
            vec![],          // Empty
            vec![0u8],       // Single zero
            vec![0xFFu8],    // Single 0xFF
            vec![0u8; 1],    // One zero
            vec![0u8; 31],   // Just under hash size
            vec![0u8; 32],   // Exactly hash size
            vec![0u8; 33],   // Just over hash size
            vec![0u8; 64],   // Common block size
            vec![0u8; 4096], // Page size
        ];

        for data in test_cases {
            let hash = hasher.hash(&data);
            // Should not panic or return errors
            assert!(hash.is_ok());
        }
    }
}

#[cfg(test)]
mod concurrent_tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    /// Test concurrent compression operations
    #[test]
    fn concurrent_compression() {
        let compressor = Arc::new(Lz4Compressor::new());
        let data = Arc::new(vec![42u8; 10240]);

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let compressor = Arc::clone(&compressor);
                let data = Arc::clone(&data);
                thread::spawn(move || {
                    for _ in 0..100 {
                        let compressed = compressor.compress(&data).unwrap();
                        let decompressed = compressor.decompress(&compressed).unwrap();
                        assert_eq!(*data, decompressed);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    /// Test concurrent hashing operations
    #[test]
    fn concurrent_hashing() {
        let hasher = Arc::new(Blake3Hasher::new());
        let data = Arc::new(vec![42u8; 10240]);

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let hasher = Arc::clone(&hasher);
                let data = Arc::clone(&data);
                thread::spawn(move || {
                    let expected_hash = hasher.hash(&data).unwrap();
                    for _ in 0..100 {
                        let hash = hasher.hash(&data).unwrap();
                        assert_eq!(hash, expected_hash);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    /// Test concurrent different operations
    #[test]
    fn concurrent_mixed_operations() {
        let compressor = Arc::new(Lz4Compressor::new());
        let hasher = Arc::new(Blake3Hasher::new());

        let handles: Vec<_> = (0..20)
            .map(|i| {
                let compressor = Arc::clone(&compressor);
                let hasher = Arc::clone(&hasher);
                thread::spawn(move || {
                    let data = vec![i as u8; 1024];

                    for _ in 0..50 {
                        // Compress
                        let compressed = compressor.compress(&data).unwrap();
                        let decompressed = compressor.decompress(&compressed).unwrap();
                        assert_eq!(data, decompressed);

                        // Hash
                        let hash1 = hasher.hash(&data).unwrap();
                        let hash2 = hasher.hash(&data).unwrap();
                        assert_eq!(hash1, hash2);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }
}

#[cfg(test)]
mod performance_regression_tests {
    use super::*;
    use std::time::Instant;

    /// Test compression performance doesn't regress
    #[test]
    fn performance_compression_speed() {
        let compressor = Lz4Compressor::new();
        let data = vec![0xAAu8; 10 * 1024 * 1024]; // 10MB

        let start = Instant::now();
        let _compressed = compressor.compress(&data).unwrap();
        let elapsed = start.elapsed();

        // LZ4 should compress 10MB in under 1 second
        assert!(
            elapsed.as_secs() < 1,
            "Compression took too long: {elapsed:?}"
        );
    }

    /// Test decompression performance
    #[test]
    fn performance_decompression_speed() {
        let compressor = Lz4Compressor::new();
        let data = vec![0xAAu8; 10 * 1024 * 1024]; // 10MB

        let compressed = compressor.compress(&data).unwrap();

        let start = Instant::now();
        let _decompressed = compressor.decompress(&compressed).unwrap();
        let elapsed = start.elapsed();

        // Decompression should be even faster than compression
        assert!(
            elapsed.as_secs() < 1,
            "Decompression took too long: {elapsed:?}"
        );
    }

    /// Test hashing performance
    #[test]
    fn performance_hashing_speed() {
        let hasher = Blake3Hasher::new();
        let data = vec![0xAAu8; 100 * 1024 * 1024]; // 100MB

        let start = Instant::now();
        let _hash = hasher.hash(&data).unwrap();
        let elapsed = start.elapsed();

        // Blake3 should hash 100MB in under 1 second
        assert!(
            elapsed.as_secs() < 1,
            "Hashing took too long: {elapsed:?}"
        );
    }

    /// Test multiple operations don't degrade over time
    #[test]
    fn performance_no_degradation() {
        let compressor = Lz4Compressor::new();
        let data = vec![0xAAu8; 1024 * 1024]; // 1MB

        let mut times = Vec::new();

        for _ in 0..100 {
            let start = Instant::now();
            let compressed = compressor.compress(&data).unwrap();
            let _decompressed = compressor.decompress(&compressed).unwrap();
            times.push(start.elapsed());
        }

        // First and last operations should have similar performance
        let first_avg: u128 = times[0..10].iter().map(std::time::Duration::as_micros).sum::<u128>() / 10;
        let last_avg: u128 = times[90..100].iter().map(std::time::Duration::as_micros).sum::<u128>() / 10;

        // Last operations should not be more than 2x slower
        assert!(
            last_avg < first_avg * 2,
            "Performance degraded: first_avg={first_avg:?}µs, last_avg={last_avg:?}µs"
        );
    }
}
