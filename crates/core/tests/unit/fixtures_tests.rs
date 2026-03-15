//! Tests for test fixtures and generators

use super::common;
use common::*;

#[test]
fn test_fixtures_create_random_data() {
    let data = create_random_data(1024);
    assert_eq!(data.len(), 1024);

    // Verify it's deterministic
    let data2 = create_random_data(1024);
    assert_eq!(data, data2);
}

#[test]
fn test_fixtures_create_sparse_data() {
    let data = create_sparse_data(10000, 0.9);
    let zero_pct = zero_percentage(&data);
    // Should be approximately 90% zeros
    assert!((zero_pct - 90.0).abs() < 5.0);
}

#[test]
fn test_fixtures_create_compressible_data() {
    let low_entropy = create_compressible_data(10000, 2.0);
    let high_entropy = create_compressible_data(10000, 7.5);

    assert_eq!(low_entropy.len(), 10000);
    assert_eq!(high_entropy.len(), 10000);

    // High entropy data should have more variation
    let entropy_low = calculate_entropy(&low_entropy);
    let entropy_high = calculate_entropy(&high_entropy);
    assert!(
        entropy_high > entropy_low,
        "Expected high > low, but got low={entropy_low:.2}, high={entropy_high:.2}"
    );
}

#[test]
fn test_fixtures_assert_bytes_equal() {
    let a = vec![1, 2, 3, 4, 5];
    let b = vec![1, 2, 3, 4, 5];
    assert_bytes_equal(&a, &b, "should be equal");
}

#[test]
fn test_fixtures_compression_ratio() {
    let ratio = measure_compression_ratio(1000, 500);
    assert_eq!(ratio, 0.5);
}

#[test]
fn test_fixtures_entropy_calculation() {
    // All zeros should have ~0 entropy
    let zeros = vec![0u8; 1000];
    assert!(calculate_entropy(&zeros) < 0.1);

    // Random data should have high entropy
    let random = create_random_data(10000);
    let entropy = calculate_entropy(&random);
    assert!(entropy > 7.0);
}

#[test]
fn test_fixtures_mock_backend() {
    let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let backend = MockBackend::new(data);

    assert_eq!(backend.len(), 10);
    assert!(!backend.is_empty());

    let mut buf = vec![0u8; 3];
    let n = backend.read(2, &mut buf).unwrap();
    assert_eq!(n, 3);
    assert_eq!(buf, vec![3, 4, 5]);
}

#[test]
fn test_fixtures_verify_pattern() {
    let data = vec![0x42; 100];
    verify_pattern(&data, 0x42);
}

#[test]
fn test_fixtures_verify_sequential() {
    let data: Vec<u8> = (0..512).map(|i| (i % 256) as u8).collect();
    verify_sequential(&data);
}

#[test]
fn test_fixtures_is_all_zeros() {
    assert!(is_all_zeros(&[0, 0, 0, 0]));
    assert!(!is_all_zeros(&[0, 1, 0, 0]));
}

#[test]
fn test_fixtures_is_all_ones() {
    assert!(is_all_ones(&[0xFF, 0xFF, 0xFF]));
    assert!(!is_all_ones(&[0xFF, 0xFE, 0xFF]));
}
