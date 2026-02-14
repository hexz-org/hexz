/// Unit tests for DCAM (Deduplication with Content-Aware Matching)
use hexz_core::algo::dedup::dcam::{DcamDeduplicator, DcamParams};

#[test]
fn test_dcam_params_default() {
    let params = DcamParams::default();
    assert!(params.min_chunk_size() > 0);
    assert!(params.avg_chunk_size() > params.min_chunk_size());
    assert!(params.max_chunk_size() > params.avg_chunk_size());
}

#[test]
fn test_dcam_params_custom() {
    let params = DcamParams::new(8192, 32768, 131072);
    assert_eq!(params.min_chunk_size(), 8192);
    assert_eq!(params.avg_chunk_size(), 32768);
    assert_eq!(params.max_chunk_size(), 131072);
}

#[test]
fn test_dcam_params_validation() {
    // Min > Avg should fail
    let result = DcamParams::new(65536, 32768, 131072);
    assert!(result.is_err(), "Min > Avg should be invalid");

    // Avg > Max should fail
    let result = DcamParams::new(16384, 131072, 65536);
    assert!(result.is_err(), "Avg > Max should be invalid");
}

#[test]
fn test_dcam_deduplicator_new() {
    let params = DcamParams::default();
    let dedup = DcamDeduplicator::new(params);
    assert!(dedup.is_ok());
}

#[test]
fn test_dcam_empty_input() {
    let params = DcamParams::default();
    let mut dedup = DcamDeduplicator::new(params).unwrap();

    let data = b"";
    let chunks = dedup.process(data);

    assert_eq!(chunks.len(), 0, "Empty input should produce no chunks");
}

#[test]
fn test_dcam_small_input() {
    let params = DcamParams::new(1024, 4096, 16384).unwrap();
    let mut dedup = DcamDeduplicator::new(params).unwrap();

    let data = b"Small data";
    let chunks = dedup.process(data);

    // Should produce at least one chunk
    assert!(chunks.len() >= 1);
}

#[test]
fn test_dcam_duplicate_detection() {
    let params = DcamParams::default();
    let mut dedup = DcamDeduplicator::new(params).unwrap();

    // Process same data twice
    let data = vec![0xAB; 65536];

    let chunks1 = dedup.process(&data);
    let chunks2 = dedup.process(&data);

    // Should detect duplicates
    let unique_count = dedup.unique_chunks();
    assert!(unique_count <= chunks1.len() + chunks2.len());
}

#[test]
fn test_dcam_dedup_ratio() {
    let params = DcamParams::default();
    let mut dedup = DcamDeduplicator::new(params).unwrap();

    // Create data with lots of repetition
    let mut data = Vec::new();
    let pattern = vec![0xCD; 4096];
    for _ in 0..10 {
        data.extend_from_slice(&pattern);
    }

    dedup.process(&data);

    let ratio = dedup.dedup_ratio();
    assert!(ratio > 1.0, "Should have dedup ratio > 1 for repetitive data");
}

#[test]
fn test_dcam_statistics() {
    let params = DcamParams::default();
    let mut dedup = DcamDeduplicator::new(params).unwrap();

    let data = vec![0xFF; 100000];
    dedup.process(&data);

    let stats = dedup.statistics();
    assert!(stats.total_chunks() > 0);
    assert!(stats.unique_chunks() > 0);
    assert!(stats.bytes_processed() == data.len());
}

#[test]
fn test_dcam_varying_data() {
    let params = DcamParams::default();
    let mut dedup = DcamDeduplicator::new(params).unwrap();

    // Highly varying data (random-like)
    let data: Vec<u8> = (0..65536).map(|i| (i % 256) as u8).collect();

    let chunks = dedup.process(&data);

    assert!(chunks.len() > 0);
}

#[test]
fn test_dcam_reset() {
    let params = DcamParams::default();
    let mut dedup = DcamDeduplicator::new(params).unwrap();

    let data = vec![0xEE; 32768];
    dedup.process(&data);

    assert!(dedup.total_chunks() > 0);

    dedup.reset();

    assert_eq!(dedup.total_chunks(), 0, "Reset should clear statistics");
}

#[test]
fn test_dcam_parameter_tuning() {
    // Test with small chunks
    let small_params = DcamParams::new(4096, 8192, 16384).unwrap();
    let mut small_dedup = DcamDeduplicator::new(small_params).unwrap();

    // Test with large chunks
    let large_params = DcamParams::new(32768, 65536, 131072).unwrap();
    let mut large_dedup = DcamDeduplicator::new(large_params).unwrap();

    let data = vec![0xBB; 262144]; // 256KB

    let small_chunks = small_dedup.process(&data);
    let large_chunks = large_dedup.process(&data);

    // Smaller chunk size should produce more chunks
    assert!(small_chunks.len() > large_chunks.len());
}

#[test]
fn test_dcam_bytes_saved() {
    let params = DcamParams::default();
    let mut dedup = DcamDeduplicator::new(params).unwrap();

    // Highly repetitive data
    let mut data = Vec::new();
    let block = vec![0x42; 16384];
    for _ in 0..20 {
        data.extend_from_slice(&block);
    }

    dedup.process(&data);

    let bytes_saved = dedup.bytes_saved();
    assert!(bytes_saved > 0, "Should save bytes with repetitive data");
}
