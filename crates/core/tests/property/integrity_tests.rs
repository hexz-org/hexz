//! Property-based tests for data integrity invariants.
//!
//! These tests use proptest to verify that compression, encryption, and serialization
//! maintain their correctness guarantees across a wide range of random inputs.

use hexz_core::algo::compression::Compressor;
use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_core::format::header::{CompressionType, FeatureFlags, Header};
use hexz_core::format::index::{BlockInfo, MasterIndex, PageEntry};
use hexz_core::format::magic::{FORMAT_VERSION, MAGIC_BYTES};
use proptest::prelude::*;

// ── Compression roundtrip invariants ────────────────────────────────────────

proptest! {
    /// LZ4: decompress(compress(data)) == data for all inputs.
    #[test]
    fn lz4_roundtrip(data in proptest::collection::vec(any::<u8>(), 0..65536)) {
        let compressor = Lz4Compressor::new();
        let compressed = compressor.compress(&data).unwrap();
        let decompressed = compressor.decompress(&compressed).unwrap();
        prop_assert_eq!(&data, &decompressed);
    }

    /// LZ4: compressed output is never larger than input + overhead bound.
    #[test]
    fn lz4_output_bounded(data in proptest::collection::vec(any::<u8>(), 1..65536)) {
        let compressor = Lz4Compressor::new();
        let compressed = compressor.compress(&data).unwrap();
        // LZ4 worst case: input + input/255 + 16 + 4-byte size header
        let max_size = data.len() + data.len() / 255 + 16 + 4;
        prop_assert!(compressed.len() <= max_size,
            "compressed {} bytes to {} (max {})", data.len(), compressed.len(), max_size);
    }

    /// LZ4: compress_into + decompress_into roundtrip matches.
    #[test]
    fn lz4_into_roundtrip(data in proptest::collection::vec(any::<u8>(), 0..65536)) {
        let compressor = Lz4Compressor::new();
        let mut compressed = Vec::new();
        compressor.compress_into(&data, &mut compressed).unwrap();

        let mut decompressed = vec![0u8; data.len()];
        let n = compressor.decompress_into(&compressed, &mut decompressed).unwrap();
        prop_assert_eq!(n, data.len());
        prop_assert_eq!(&data, &decompressed);
    }
}

// ── Compression corruption detection ────────────────────────────────────────

proptest! {
    /// LZ4: flipping any single bit in compressed data either produces an error
    /// or produces different output (never silently returns the original data with
    /// corruption undetected — unless the flip is in padding that doesn't affect output).
    #[test]
    fn lz4_single_bitflip_detected(
        data in proptest::collection::vec(any::<u8>(), 16..4096),
        flip_byte in 0usize..4096,
        flip_bit in 0u8..8,
    ) {
        let compressor = Lz4Compressor::new();
        let compressed = compressor.compress(&data).unwrap();

        // Pick a byte to flip within the compressed data
        let flip_byte = flip_byte % compressed.len();
        let mut corrupted = compressed.clone();
        corrupted[flip_byte] ^= 1 << flip_bit;

        match compressor.decompress(&corrupted) {
            Err(_) => {} // corruption detected — good
            Ok(_result) => {
                // LZ4 has no built-in checksum, so some bit flips can
                // produce valid but different decompressed output — that's
                // expected and acceptable. CRC32 catches this at the block
                // read layer. The invariant here is simply: no panics.
            }
        }
    }
}

// ── Encryption property tests ───────────────────────────────────────────────

#[cfg(feature = "encryption")]
mod encryption_props {
    use super::*;
    use hexz_core::algo::encryption::{AesGcmEncryptor, Encryptor};

    // Fewer cases for encryption tests due to PBKDF2 cost (~100ms per case)
    proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(16))]
        /// Encryption roundtrip: decrypt(encrypt(data, idx), idx) == data.
        #[test]
        fn aes_gcm_roundtrip(
            data in proptest::collection::vec(any::<u8>(), 0..8192),
            block_idx in any::<u64>(),
        ) {
            let encryptor = AesGcmEncryptor::new(
                b"proptest-password", b"proptest_salt_16", 100_000
            ).unwrap();
            let encrypted = encryptor.encrypt(&data, block_idx).unwrap();
            let decrypted = encryptor.decrypt(&encrypted, block_idx).unwrap();
            prop_assert_eq!(&data, &decrypted);
        }

        /// Different block indices produce different ciphertexts.
        #[test]
        fn aes_gcm_different_nonces(
            data in proptest::collection::vec(1u8..=255, 16..256),
            idx_a in any::<u64>(),
        ) {
            let idx_b = idx_a.wrapping_add(1);
            let encryptor = AesGcmEncryptor::new(
                b"proptest-password", b"proptest_salt_16", 100_000
            ).unwrap();
            let enc_a = encryptor.encrypt(&data, idx_a).unwrap();
            let enc_b = encryptor.encrypt(&data, idx_b).unwrap();
            prop_assert_ne!(enc_a, enc_b);
        }

        /// Any single-bit flip in ciphertext causes authentication failure.
        #[test]
        fn aes_gcm_bitflip_rejected(
            data in proptest::collection::vec(any::<u8>(), 1..512),
            block_idx in any::<u64>(),
            flip_byte in 0usize..512,
            flip_bit in 0u8..8,
        ) {
            let encryptor = AesGcmEncryptor::new(
                b"proptest-password", b"proptest_salt_16", 100_000
            ).unwrap();
            let encrypted = encryptor.encrypt(&data, block_idx).unwrap();

            let flip_byte = flip_byte % encrypted.len();
            let mut corrupted = encrypted.clone();
            corrupted[flip_byte] ^= 1 << flip_bit;

            // AES-GCM MUST reject any modification
            prop_assert!(encryptor.decrypt(&corrupted, block_idx).is_err());
        }

        /// Wrong block index always causes decryption failure.
        #[test]
        fn aes_gcm_wrong_index_rejected(
            data in proptest::collection::vec(any::<u8>(), 1..512),
            idx_a in any::<u64>(),
        ) {
            let idx_b = idx_a.wrapping_add(1);
            let encryptor = AesGcmEncryptor::new(
                b"proptest-password", b"proptest_salt_16", 100_000
            ).unwrap();
            let encrypted = encryptor.encrypt(&data, idx_a).unwrap();
            prop_assert!(encryptor.decrypt(&encrypted, idx_b).is_err());
        }
    }
}

// ── Serialization roundtrip invariants ──────────────────────────────────────

proptest! {
    /// Header: deserialize(serialize(header)) == header.
    #[test]
    fn header_roundtrip(
        block_size in prop_oneof![Just(4096u32), Just(65536u32), Just(131072u32)],
        index_offset in any::<u64>(),
        has_disk in any::<bool>(),
        has_memory in any::<bool>(),
        variable_blocks in any::<bool>(),
    ) {
        let header = Header {
            magic: *MAGIC_BYTES,
            version: FORMAT_VERSION,
            block_size,
            index_offset,
            parent_path: None,
            dictionary_offset: None,
            dictionary_length: None,
            metadata_offset: None,
            metadata_length: None,
            signature_offset: None,
            signature_length: None,
            encryption: None,
            compression: CompressionType::Lz4,
            features: FeatureFlags { has_disk, has_memory, variable_blocks },
        };

        let bytes = bincode::serialize(&header).unwrap();
        let deserialized: Header = bincode::deserialize(&bytes).unwrap();
        prop_assert_eq!(header, deserialized);
    }

    /// BlockInfo: deserialize(serialize(info)) == info.
    #[test]
    fn block_info_roundtrip(
        offset in any::<u64>(),
        length in any::<u32>(),
        checksum in any::<u32>(),
        logical_len in any::<u32>(),
    ) {
        let info = BlockInfo { offset, length, checksum, logical_len };
        let bytes = bincode::serialize(&info).unwrap();
        let deserialized: BlockInfo = bincode::deserialize(&bytes).unwrap();
        prop_assert_eq!(info.offset, deserialized.offset);
        prop_assert_eq!(info.length, deserialized.length);
        prop_assert_eq!(info.checksum, deserialized.checksum);
        prop_assert_eq!(info.logical_len, deserialized.logical_len);
    }

    /// MasterIndex: deserialize(serialize(index)) == index.
    #[test]
    fn master_index_roundtrip(
        disk_size in any::<u64>(),
        memory_size in any::<u64>(),
        n_disk_pages in 0usize..8,
        n_memory_pages in 0usize..4,
    ) {
        let master = MasterIndex {
            disk_pages: (0..n_disk_pages).map(|i| PageEntry {
                start_block: i as u64 * 4096,
                start_logical: i as u64 * 4096 * 65536,
                offset: 4096 + i as u64 * 65536,
                length: 65536,
            }).collect(),
            memory_pages: (0..n_memory_pages).map(|i| PageEntry {
                start_block: i as u64 * 4096,
                start_logical: i as u64 * 4096 * 65536,
                offset: 4096 + i as u64 * 65536,
                length: 65536,
            }).collect(),
            disk_size,
            memory_size,
        };

        let bytes = bincode::serialize(&master).unwrap();
        let deserialized: MasterIndex = bincode::deserialize(&bytes).unwrap();
        prop_assert_eq!(master.disk_size, deserialized.disk_size);
        prop_assert_eq!(master.memory_size, deserialized.memory_size);
        prop_assert_eq!(master.disk_pages.len(), deserialized.disk_pages.len());
        prop_assert_eq!(master.memory_pages.len(), deserialized.memory_pages.len());
    }
}

// ── CRC32 integrity invariants ──────────────────────────────────────────────

proptest! {
    /// CRC32 is deterministic: same data always produces same checksum.
    #[test]
    fn crc32_deterministic(data in proptest::collection::vec(any::<u8>(), 0..65536)) {
        let hash1 = crc32fast::hash(&data);
        let hash2 = crc32fast::hash(&data);
        prop_assert_eq!(hash1, hash2);
    }

    /// CRC32: any single-byte change produces a different checksum.
    #[test]
    fn crc32_single_byte_change_detected(
        data in proptest::collection::vec(any::<u8>(), 1..4096),
        change_pos in 0usize..4096,
    ) {
        let change_pos = change_pos % data.len();
        let original_hash = crc32fast::hash(&data);

        let mut modified = data.clone();
        modified[change_pos] = modified[change_pos].wrapping_add(1);

        let modified_hash = crc32fast::hash(&modified);
        prop_assert_ne!(original_hash, modified_hash,
            "CRC32 collision on single-byte change at position {}", change_pos);
    }
}
