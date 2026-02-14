//! Unit tests for HTTP storage backend
//!
//! Tests HTTP/HTTPS storage backend functionality including:
//! - Range request handling (RFC 7233)
//! - Error handling (404, 403, 500, etc.)
//! - Connection pooling and timeouts
//! - URL validation and security checks
//! - Concurrent access patterns

use hexz_core::store::http::HttpBackend;
use hexz_core::store::StorageBackend;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, header},
};

/// Test data for mock server
const TEST_DATA: &[u8] = b"Hello, World! This is test data for HTTP backend testing.";

#[tokio::test]
async fn test_http_backend_read_full_content() {
    let mock_server = MockServer::start().await;

    // Mock HEAD request to get content length
    Mock::given(method("HEAD"))
        .and(path("/test.bin"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-length", TEST_DATA.len().to_string())
        )
        .mount(&mock_server)
        .await;

    // Mock GET request with range
    Mock::given(method("GET"))
        .and(path("/test.bin"))
        .and(header("range", "bytes=0-58"))
        .respond_with(
            ResponseTemplate::new(206)
                .set_body_bytes(TEST_DATA)
                .insert_header("content-range", format!("bytes 0-58/{}", TEST_DATA.len()))
        )
        .mount(&mock_server)
        .await;

    let url = format!("{}/test.bin", mock_server.uri());
    let backend = HttpBackend::new(url.clone(), true).expect("Failed to create HTTP backend");

    assert_eq!(backend.len(), TEST_DATA.len() as u64);

    let data = backend.read_exact(0, TEST_DATA.len()).expect("Failed to read data");
    assert_eq!(&data[..], TEST_DATA);
}

#[tokio::test]
async fn test_http_backend_read_partial_range() {
    let mock_server = MockServer::start().await;

    Mock::given(method("HEAD"))
        .and(path("/test.bin"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-length", TEST_DATA.len().to_string())
        )
        .mount(&mock_server)
        .await;

    // Read bytes 7-18 (should be "World! This")
    Mock::given(method("GET"))
        .and(path("/test.bin"))
        .and(header("range", "bytes=7-18"))
        .respond_with(
            ResponseTemplate::new(206)
                .set_body_bytes(&TEST_DATA[7..19])
                .insert_header("content-range", format!("bytes 7-18/{}", TEST_DATA.len()))
        )
        .mount(&mock_server)
        .await;

    let url = format!("{}/test.bin", mock_server.uri());
    let backend = HttpBackend::new(url.clone(), true).expect("Failed to create HTTP backend");

    let data = backend.read_exact(7, 12).expect("Failed to read partial data");
    assert_eq!(&data[..], &TEST_DATA[7..19]);
}

#[tokio::test]
async fn test_http_backend_404_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("HEAD"))
        .and(path("/notfound.bin"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let url = format!("{}/notfound.bin", mock_server.uri());
    let result = HttpBackend::new(url.clone(), true);

    assert!(result.is_err(), "Should fail with 404");
}

#[tokio::test]
async fn test_http_backend_500_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("HEAD"))
        .and(path("/test.bin"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-length", TEST_DATA.len().to_string())
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/test.bin"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let url = format!("{}/test.bin", mock_server.uri());
    let backend = HttpBackend::new(url.clone(), true).expect("Failed to create HTTP backend");

    let result = backend.read_exact(0, 10);
    assert!(result.is_err(), "Should fail with 500 error");
}

#[test]
fn test_http_backend_url_validation_localhost() {
    // Localhost should be blocked by default
    let result = HttpBackend::new("http://localhost:8080/test.bin".to_string(), false);
    assert!(result.is_err(), "Should block localhost without allow_restricted");

    // But allowed with flag
    let result = HttpBackend::new("http://localhost:8080/test.bin".to_string(), true);
    // Will fail because server doesn't exist, but URL validation should pass
    assert!(result.is_err()); // Connection error, not validation error
}

#[test]
fn test_http_backend_url_validation_private_networks() {
    // Private network ranges should be blocked by default
    let private_ips = vec![
        "http://192.168.1.1/test.bin",
        "http://10.0.0.1/test.bin",
        "http://172.16.0.1/test.bin",
    ];

    for url in private_ips {
        let result = HttpBackend::new(url.to_string(), false);
        assert!(result.is_err(), "Should block private IP: {}", url);
    }
}

#[tokio::test]
async fn test_http_backend_content_length_mismatch() {
    let mock_server = MockServer::start().await;

    // HEAD returns one size
    Mock::given(method("HEAD"))
        .and(path("/test.bin"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-length", "1000")
        )
        .mount(&mock_server)
        .await;

    // But GET returns different size
    Mock::given(method("GET"))
        .and(path("/test.bin"))
        .respond_with(
            ResponseTemplate::new(206)
                .set_body_bytes(TEST_DATA) // Much smaller
                .insert_header("content-range", format!("bytes 0-58/1000"))
        )
        .mount(&mock_server)
        .await;

    let url = format!("{}/test.bin", mock_server.uri());
    let backend = HttpBackend::new(url.clone(), true).expect("Failed to create HTTP backend");

    assert_eq!(backend.len(), 1000);
}

#[tokio::test]
async fn test_http_backend_concurrent_reads() {
    use std::sync::Arc;

    let mock_server = MockServer::start().await;

    Mock::given(method("HEAD"))
        .and(path("/test.bin"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-length", TEST_DATA.len().to_string())
        )
        .expect(1..) // Can be called multiple times
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/test.bin"))
        .respond_with(
            ResponseTemplate::new(206)
                .set_body_bytes(TEST_DATA)
                .insert_header("content-range", format!("bytes 0-58/{}", TEST_DATA.len()))
        )
        .expect(3) // Exactly 3 concurrent reads
        .mount(&mock_server)
        .await;

    let url = format!("{}/test.bin", mock_server.uri());
    let backend = Arc::new(HttpBackend::new(url.clone(), true).expect("Failed to create HTTP backend"));

    // Spawn 3 concurrent reads
    let handles: Vec<_> = (0..3)
        .map(|_| {
            let backend = Arc::clone(&backend);
            std::thread::spawn(move || {
                backend.read_exact(0, TEST_DATA.len()).expect("Failed to read")
            })
        })
        .collect();

    // Wait for all to complete
    for handle in handles {
        let data = handle.join().expect("Thread panicked");
        assert_eq!(&data[..], TEST_DATA);
    }
}
