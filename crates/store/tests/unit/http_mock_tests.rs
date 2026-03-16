//! HTTP backend tests with a real mock HTTP server.
//!
//! Uses `std::net::TcpListener` for a lightweight mock server to test
//! the sync HTTP backend's `read_exact` and len methods.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;

use hexz_store::StorageBackend;
use hexz_store::http::HttpBackend;

/// Start a simple HTTP server that responds to HEAD and GET requests.
/// Returns (address, `join_handle`).
fn start_mock_server(data: Vec<u8>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}/archive.hxz", addr.port());

    let handle = thread::spawn(move || {
        // Handle up to 20 requests then exit
        for _ in 0..20 {
            let Ok((stream, _)) = listener.accept() else {
                break;
            };
            handle_request(stream, &data);
        }
    });

    // Give the server a moment to start
    thread::sleep(std::time::Duration::from_millis(10));

    (url, handle)
}

fn handle_request(stream: std::net::TcpStream, data: &[u8]) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;

    // Read request line
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }

    // Read all headers
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            return;
        }
        if line.trim().is_empty() {
            break;
        }
        headers.push(line);
    }

    let is_head = request_line.starts_with("HEAD");
    let is_get = request_line.starts_with("GET");

    if is_head {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
            data.len()
        );
        let _ = writer.write_all(response.as_bytes());
    } else if is_get {
        // Parse Range header (case-insensitive)
        let range_header = headers
            .iter()
            .find(|h| h.to_lowercase().starts_with("range:"));

        if let Some(range_line) = range_header {
            // Parse "range: bytes=start-end"
            let lower = range_line.to_lowercase();
            if let Some(range_val) = lower.strip_prefix("range: bytes=") {
                let range_val = range_val.trim();
                let parts: Vec<&str> = range_val.split('-').collect();
                if parts.len() == 2 {
                    let start: usize = parts[0].parse().unwrap_or(0);
                    let end: usize = parts[1].parse().unwrap_or(data.len() - 1);
                    let end = end.min(data.len() - 1);
                    let slice = &data[start..=end];

                    let response = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
                        slice.len(),
                        start,
                        end,
                        data.len()
                    );
                    let _ = writer.write_all(response.as_bytes());
                    let _ = writer.write_all(slice);
                    return;
                }
            }
        }

        // Full GET response (no Range header)
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            data.len()
        );
        let _ = writer.write_all(response.as_bytes());
        let _ = writer.write_all(data);
    } else {
        let response = "HTTP/1.1 405 Method Not Allowed\r\nConnection: close\r\n\r\n";
        let _ = writer.write_all(response.as_bytes());
    }
}

/// Test creating an HTTP backend with a real server.
#[test]
fn test_http_backend_with_mock_server() {
    let data = vec![0x42u8; 1024];
    let (url, _handle) = start_mock_server(data);

    // allow_restricted=true since we're connecting to localhost
    let backend = HttpBackend::new(&url, true).unwrap();

    assert_eq!(backend.len(), 1024);
}

/// Test reading exact bytes from HTTP backend.
#[test]
fn test_http_backend_read_exact() {
    let data: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();
    let (url, _handle) = start_mock_server(data.clone());

    let backend = HttpBackend::new(&url, true).unwrap();

    // Read first 100 bytes
    let result = backend.read_exact(0, 100).unwrap();
    assert_eq!(result.len(), 100);
    assert_eq!(&result[..], &data[0..100]);
}

/// Test reading from various offsets.
#[test]
fn test_http_backend_read_various_offsets() {
    let data: Vec<u8> = (0..8192).map(|i| (i % 256) as u8).collect();
    let (url, _handle) = start_mock_server(data.clone());

    let backend = HttpBackend::new(&url, true).unwrap();

    // Read from middle
    let result = backend.read_exact(1000, 500).unwrap();
    assert_eq!(result.len(), 500);
    assert_eq!(&result[..], &data[1000..1500]);
}

/// Test HTTP backend `len()` returns correct size.
#[test]
fn test_http_backend_len() {
    let data = vec![0xAA; 4096];
    let (url, _handle) = start_mock_server(data);

    let backend = HttpBackend::new(&url, true).unwrap();
    assert_eq!(backend.len(), 4096);
}

/// Test reading the full file through HTTP.
#[test]
fn test_http_backend_full_read() {
    let data: Vec<u8> = (0..2048).map(|i| (i % 256) as u8).collect();
    let (url, _handle) = start_mock_server(data.clone());

    let backend = HttpBackend::new(&url, true).unwrap();

    let result = backend.read_exact(0, 2048).unwrap();
    assert_eq!(result.len(), 2048);
    assert_eq!(&result[..], &data[..]);
}

/// Test that HTTP backend is Send + Sync.
#[test]
fn test_http_backend_send_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<HttpBackend>();
    assert_sync::<HttpBackend>();
}

/// Test HTTP backend with missing Content-Length header.
#[test]
fn test_http_backend_missing_content_length() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}/archive.hxz", addr.port());

    let _handle = thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut writer = stream;
            // Read full request
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                    break;
                }
            }
            // Respond without Content-Length
            let response = "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n";
            let _ = writer.write_all(response.as_bytes());
        }
    });

    thread::sleep(std::time::Duration::from_millis(10));

    let result = HttpBackend::new(&url, true);
    assert!(result.is_err(), "Should fail without Content-Length header");
}

/// Test HTTP backend with 404 response.
#[test]
fn test_http_backend_404_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://127.0.0.1:{}/not_found.hxz", addr.port());

    let _handle = thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut writer = stream;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                    break;
                }
            }
            let response =
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = writer.write_all(response.as_bytes());
        }
    });

    thread::sleep(std::time::Duration::from_millis(10));

    let result = HttpBackend::new(&url, true);
    assert!(result.is_err(), "Should fail with 404 response");
}

/// Test HTTP backend error status during `read_exact`.
#[test]
fn test_http_backend_error_status_on_read() {
    let data = vec![0x42u8; 1024];
    let (url, _handle) = start_mock_server(data);

    let backend = HttpBackend::new(&url, true).unwrap();

    // Now spin up a server that returns 500 for GET requests.
    // We can't easily change the running server, so instead we test
    // that the backend correctly propagates HTTP errors by trying to
    // read beyond the data length which will cause a short read error.
    let result = backend.read_exact(0, 2048);
    assert!(
        result.is_err(),
        "Should fail when server returns fewer bytes than requested"
    );
}

/// Test HTTP backend short read (server returns fewer bytes than requested).
#[test]
fn test_http_backend_short_read() {
    // Create a server with 512 bytes of data
    let data = vec![0xBB; 512];
    let (url, _handle) = start_mock_server(data);

    let backend = HttpBackend::new(&url, true).unwrap();
    assert_eq!(backend.len(), 512);

    // Request more bytes than available — the range will be clamped
    // by the mock server, resulting in fewer bytes returned than requested.
    let result = backend.read_exact(256, 512);
    assert!(
        result.is_err(),
        "Should fail when server returns fewer bytes"
    );
}
