//! Unit tests for format module - header, index, and magic bytes.
//!
//! Tests validate correct serialization/deserialization of all format structures,
//! ensuring forward/backward compatibility and handling of corrupt data.

use hexz_common::crypto::KeyDerivationParams;
use hexz_core::format::{
    header::{CompressionType, FeatureFlags, Header},
    index::{BlockInfo, ENTRIES_PER_PAGE, IndexPage, MasterIndex, PageEntry},
    magic::{FORMAT_VERSION, HEADER_SIZE, MAGIC_BYTES},
};

#[test]
fn test_magic_bytes_constant() {
    assert_eq!(MAGIC_BYTES.len(), 4);
    assert_eq!(&MAGIC_BYTES[..], b"HEXZ");
}

#[test]
fn test_header_size_constant() {
    assert_eq!(HEADER_SIZE, 4096);
}

#[test]
fn test_format_version() {
    assert_eq!(FORMAT_VERSION, 1);
}

/// Test header serialization round-trip with minimal configuration.
#[test]
fn test_header_serialization_minimal() {
    let header = Header {
        magic: *MAGIC_BYTES,
        version: FORMAT_VERSION,
        block_size: 65536,
        index_offset: 1024,
        parent_path: None,
        dictionary_offset: None,
        dictionary_length: None,
        metadata_offset: None,
        metadata_length: None,
        signature_offset: None,
        signature_length: None,
        encryption: None,
        compression: CompressionType::Lz4,
        features: FeatureFlags {
            has_disk: true,
            has_memory: false,
            variable_blocks: false,
        },
    };

    let serialized = bincode::serialize(&header).expect("Failed to serialize header");
    let deserialized: Header =
        bincode::deserialize(&serialized).expect("Failed to deserialize header");

    assert_eq!(deserialized.magic, *MAGIC_BYTES);
    assert_eq!(deserialized.version, FORMAT_VERSION);
    assert_eq!(deserialized.block_size, 65536);
    assert_eq!(deserialized.index_offset, 1024);
    assert!(matches!(deserialized.compression, CompressionType::Lz4));
    assert!(deserialized.features.has_disk);
    assert!(!deserialized.features.has_memory);
}

/// Test header with all optional fields populated.
#[test]
fn test_header_serialization_full() {
    let header = Header {
        magic: *MAGIC_BYTES,
        version: FORMAT_VERSION,
        block_size: 16384,
        index_offset: 999999,
        parent_path: Some("/path/to/parent.hxz".to_string()),
        dictionary_offset: Some(5000),
        dictionary_length: Some(4096),
        metadata_offset: Some(10000),
        metadata_length: Some(512),
        signature_offset: Some(20000),
        signature_length: Some(64),
        encryption: Some(KeyDerivationParams {
            salt: [42u8; 16],
            iterations: 100000,
        }),
        compression: CompressionType::Zstd,
        features: FeatureFlags {
            has_disk: true,
            has_memory: true,
            variable_blocks: true,
        },
    };

    let serialized = bincode::serialize(&header).unwrap();
    let deserialized: Header = bincode::deserialize(&serialized).unwrap();

    assert_eq!(
        deserialized.parent_path,
        Some("/path/to/parent.hxz".to_string())
    );
    assert_eq!(deserialized.dictionary_offset, Some(5000));
    assert_eq!(deserialized.signature_length, Some(64));
    assert!(deserialized.encryption.is_some());
    assert_eq!(deserialized.encryption.unwrap().salt, [42u8; 16]);
}

/// Test BlockInfo structure integrity.
#[test]
fn test_block_info_structure() {
    let block = BlockInfo {
        hash: [0u8; 32],
        offset: 4096,
        length: 2048,
        logical_len: 65536,
        checksum: 0x12345678,
    };

    let serialized = bincode::serialize(&block).unwrap();
    let deserialized: BlockInfo = bincode::deserialize(&serialized).unwrap();

    assert_eq!(deserialized.offset, 4096);
    assert_eq!(deserialized.length, 2048);
    assert_eq!(deserialized.logical_len, 65536);
    assert_eq!(deserialized.checksum, 0x12345678);
}

/// Test zero block encoding (offset=0, length=0).
#[test]
fn test_block_info_zero_block() {
    let zero_block = BlockInfo {
        hash: [0u8; 32],
        offset: 0,
        length: 0,
        logical_len: 65536,
        checksum: 0,
    };

    assert_eq!(zero_block.offset, 0);
    assert_eq!(zero_block.length, 0);
    assert!(zero_block.logical_len > 0);
}

/// Test parent block encoding (offset=u64::MAX).
#[test]
fn test_block_info_parent_reference() {
    const BLOCK_OFFSET_PARENT: u64 = u64::MAX;

    let parent_block = BlockInfo {
        hash: [0u8; 32],
        offset: BLOCK_OFFSET_PARENT,
        length: 0,
        logical_len: 65536,
        checksum: 0,
    };

    assert_eq!(parent_block.offset, u64::MAX);
}

/// Test IndexPage with maximum entries.
#[test]
fn test_index_page_max_entries() {
    let mut page = IndexPage { blocks: vec![] };

    for i in 0..ENTRIES_PER_PAGE {
        page.blocks.push(BlockInfo {
            hash: [0u8; 32],
            offset: i as u64 * 1000,
            length: 500,
            logical_len: 65536,
            checksum: i as u32,
        });
    }

    assert_eq!(page.blocks.len(), ENTRIES_PER_PAGE);

    let serialized = bincode::serialize(&page).unwrap();
    let deserialized: IndexPage = bincode::deserialize(&serialized).unwrap();

    assert_eq!(deserialized.blocks.len(), ENTRIES_PER_PAGE);
    assert_eq!(deserialized.blocks[0].offset, 0);
    assert_eq!(
        deserialized.blocks[ENTRIES_PER_PAGE - 1].checksum,
        (ENTRIES_PER_PAGE - 1) as u32
    );
}

/// Test PageEntry structure.
#[test]
fn test_page_entry() {
    let entry = PageEntry {
        offset: 1000000,
        length: 24576,
        start_block: 128,
        start_logical: 8388608,
    };

    let serialized = bincode::serialize(&entry).unwrap();
    let deserialized: PageEntry = bincode::deserialize(&serialized).unwrap();

    assert_eq!(deserialized.offset, 1000000);
    assert_eq!(deserialized.length, 24576);
    assert_eq!(deserialized.start_block, 128);
    assert_eq!(deserialized.start_logical, 8388608);
}

/// Test MasterIndex with multiple pages.
#[test]
fn test_master_index_multi_page() {
    let mut master = MasterIndex {
        primary_size: 10737418240,  // 10 GB
        secondary_size: 4294967296, // 4 GB
        primary_pages: vec![],
        secondary_pages: vec![],
    };

    // Add 10 disk pages
    for i in 0..10 {
        master.primary_pages.push(PageEntry {
            offset: i * 100000,
            length: 24576,
            start_block: i * 1024,
            start_logical: i * 67108864,
        });
    }

    // Add 5 memory pages
    for i in 0..5 {
        master.secondary_pages.push(PageEntry {
            offset: 2000000 + i * 50000,
            length: 20480,
            start_block: i * 1024,
            start_logical: i * 67108864,
        });
    }

    let serialized = bincode::serialize(&master).unwrap();
    let deserialized: MasterIndex = bincode::deserialize(&serialized).unwrap();

    assert_eq!(deserialized.primary_size, 10737418240);
    assert_eq!(deserialized.secondary_size, 4294967296);
    assert_eq!(deserialized.primary_pages.len(), 10);
    assert_eq!(deserialized.secondary_pages.len(), 5);
}

/// Test MasterIndex with empty pages (edge case).
#[test]
fn test_master_index_empty() {
    let master = MasterIndex {
        primary_size: 0,
        secondary_size: 0,
        primary_pages: vec![],
        secondary_pages: vec![],
    };

    let serialized = bincode::serialize(&master).unwrap();
    let deserialized: MasterIndex = bincode::deserialize(&serialized).unwrap();

    assert_eq!(deserialized.primary_size, 0);
    assert!(deserialized.primary_pages.is_empty());
    assert!(deserialized.secondary_pages.is_empty());
}

/// Test compression type enum.
#[test]
fn test_compression_type_variants() {
    let lz4 = CompressionType::Lz4;
    let zstd = CompressionType::Zstd;

    let lz4_ser = bincode::serialize(&lz4).unwrap();
    let zstd_ser = bincode::serialize(&zstd).unwrap();

    let lz4_de: CompressionType = bincode::deserialize(&lz4_ser).unwrap();
    let zstd_de: CompressionType = bincode::deserialize(&zstd_ser).unwrap();

    assert!(matches!(lz4_de, CompressionType::Lz4));
    assert!(matches!(zstd_de, CompressionType::Zstd));
}

/// Test feature flags all combinations.
#[test]
fn test_feature_flags_combinations() {
    let test_cases = vec![
        (true, true, true),
        (true, false, false),
        (false, true, false),
        (false, false, true),
        (false, false, false),
    ];

    for (has_disk, has_memory, variable_blocks) in test_cases {
        let flags = FeatureFlags {
            has_disk,
            has_memory,
            variable_blocks,
        };

        let serialized = bincode::serialize(&flags).unwrap();
        let deserialized: FeatureFlags = bincode::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.has_disk, has_disk);
        assert_eq!(deserialized.has_memory, has_memory);
        assert_eq!(deserialized.variable_blocks, variable_blocks);
    }
}

/// Test encryption key derivation parameters.
#[test]
fn test_key_derivation_params() {
    let params = KeyDerivationParams {
        salt: [0xAB; 16],
        iterations: 100000,
    };

    let serialized = bincode::serialize(&params).unwrap();
    let deserialized: KeyDerivationParams = bincode::deserialize(&serialized).unwrap();

    assert_eq!(deserialized.salt, [0xAB; 16]);
    assert_eq!(deserialized.iterations, 100000);
}

/// Test that corrupted magic bytes are detectable.
#[test]
fn test_invalid_magic_bytes() {
    let mut invalid_header = Header {
        magic: *MAGIC_BYTES,
        version: FORMAT_VERSION,
        block_size: 65536,
        index_offset: 1024,
        parent_path: None,
        dictionary_offset: None,
        dictionary_length: None,
        metadata_offset: None,
        metadata_length: None,
        signature_offset: None,
        signature_length: None,
        encryption: None,
        compression: CompressionType::Lz4,
        features: FeatureFlags {
            has_disk: true,
            has_memory: false,
            variable_blocks: false,
        },
    };

    invalid_header.magic = [0xFF; 4];

    // Serialize the invalid header
    let serialized = bincode::serialize(&invalid_header).unwrap();
    let deserialized: Header = bincode::deserialize(&serialized).unwrap();

    // Verify magic bytes are incorrect
    assert_ne!(deserialized.magic, *MAGIC_BYTES);
}

/// Test large block sizes (edge case).
#[test]
fn test_large_block_sizes() {
    let block_sizes = vec![16384, 65536, 262144, 1048576, 4194304];

    for size in block_sizes {
        let header = Header {
            magic: *MAGIC_BYTES,
            version: FORMAT_VERSION,
            block_size: size,
            index_offset: 1024,
            parent_path: None,
            dictionary_offset: None,
            dictionary_length: None,
            metadata_offset: None,
            metadata_length: None,
            signature_offset: None,
            signature_length: None,
            encryption: None,
            compression: CompressionType::Lz4,
            features: FeatureFlags {
                has_disk: true,
                has_memory: false,
                variable_blocks: false,
            },
        };

        let serialized = bincode::serialize(&header).unwrap();
        let deserialized: Header = bincode::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.block_size, size);
    }
}

/// Test index with huge number of pages (stress test).
#[test]
fn test_master_index_large_scale() {
    let mut master = MasterIndex {
        primary_size: 1099511627776, // 1 TB
        secondary_size: 0,
        primary_pages: vec![],
        secondary_pages: vec![],
    };

    // Add 10,000 pages (simulating 1TB snapshot)
    for i in 0..10000 {
        master.primary_pages.push(PageEntry {
            offset: i * 1000000,
            length: 24576,
            start_block: i * 1024,
            start_logical: i * 67108864,
        });
    }

    let serialized = bincode::serialize(&master).unwrap();
    let deserialized: MasterIndex = bincode::deserialize(&serialized).unwrap();

    assert_eq!(deserialized.primary_pages.len(), 10000);
    assert_eq!(deserialized.primary_pages[9999].start_block, 9999 * 1024);
}
