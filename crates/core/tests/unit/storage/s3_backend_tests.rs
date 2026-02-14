//! S3 backend tests with a mock S3 server.
//!
//! Uses `std::net::TcpListener` for a lightweight mock that speaks enough
//! of the S3 REST API (path-style) to exercise `S3Backend`.

#[cfg(feature = "s3")]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;

    use hexz_core::store::StorageBackend;
    use hexz_core::store::s3::S3Backend;

    /// Start a mock S3 server that handles HEAD and GET with Range on a single
    /// path-style object `/bucket/key`.
    fn start_mock_s3(data: Vec<u8>) -> (String, u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let handle = thread::spawn(move || {
            for _ in 0..30 {
                let stream = match listener.accept() {
                    Ok((s, _)) => s,
                    Err(_) => break,
                };
                handle_s3_request(stream, &data);
            }
        });

        thread::sleep(std::time::Duration::from_millis(20));

        let endpoint = format!("http://127.0.0.1:{}", port);
        (endpoint, port, handle)
    }

    fn handle_s3_request(stream: std::net::TcpStream, data: &[u8]) {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;

        let mut request_line = String::new();
        if reader.read_line(&mut request_line).is_err() {
            return;
        }

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

        // Check the path — we serve /bucket/key and nothing else
        let path = request_line.split_whitespace().nth(1).unwrap_or("/");

        let is_our_object = path.contains("/test-bucket/test-key");

        if !is_our_object {
            let response =
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = writer.write_all(response.as_bytes());
            return;
        }

        if is_head {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                data.len()
            );
            let _ = writer.write_all(response.as_bytes());
        } else if is_get {
            let range_header = headers
                .iter()
                .find(|h| h.to_lowercase().starts_with("range:"));

            if let Some(range_line) = range_header {
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

            // Full GET
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

    /// Helper to create an S3Backend pointing at the mock server.
    fn make_backend(endpoint: &str) -> S3Backend {
        // Set dummy credentials so rust-s3 doesn't fail on missing env
        // SAFETY: Tests run serially for S3 tests; no other thread reads these env vars concurrently.
        unsafe {
            std::env::set_var("AWS_ACCESS_KEY_ID", "testing");
            std::env::set_var("AWS_SECRET_ACCESS_KEY", "testing");
        }

        S3Backend::new(
            "test-bucket".to_string(),
            "test-key".to_string(),
            "us-east-1".to_string(),
            Some(endpoint.to_string()),
        )
        .expect("Failed to create S3Backend")
    }

    #[test]
    fn test_s3_backend_construction() {
        let data = vec![0x42u8; 2048];
        let (endpoint, _, _handle) = start_mock_s3(data);
        let backend = make_backend(&endpoint);
        assert_eq!(backend.len(), 2048);
    }

    #[test]
    fn test_s3_backend_read_exact() {
        let data: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();
        let (endpoint, _, _handle) = start_mock_s3(data.clone());
        let backend = make_backend(&endpoint);

        let result = backend.read_exact(0, 100).unwrap();
        assert_eq!(result.len(), 100);
        assert_eq!(&result[..], &data[0..100]);
    }

    #[test]
    fn test_s3_backend_read_middle() {
        let data: Vec<u8> = (0..8192).map(|i| (i % 256) as u8).collect();
        let (endpoint, _, _handle) = start_mock_s3(data.clone());
        let backend = make_backend(&endpoint);

        let result = backend.read_exact(1000, 500).unwrap();
        assert_eq!(result.len(), 500);
        assert_eq!(&result[..], &data[1000..1500]);
    }

    #[test]
    fn test_s3_backend_len() {
        let data = vec![0xAA; 4096];
        let (endpoint, _, _handle) = start_mock_s3(data);
        let backend = make_backend(&endpoint);
        assert_eq!(backend.len(), 4096);
    }

    #[test]
    fn test_s3_backend_404() {
        let data = vec![0u8; 100];
        let (endpoint, _, _handle) = start_mock_s3(data);

        // SAFETY: Tests run serially for S3 tests; no other thread reads these env vars concurrently.
        unsafe {
            std::env::set_var("AWS_ACCESS_KEY_ID", "testing");
            std::env::set_var("AWS_SECRET_ACCESS_KEY", "testing");
        }

        let result = S3Backend::new(
            "wrong-bucket".to_string(),
            "wrong-key".to_string(),
            "us-east-1".to_string(),
            Some(endpoint),
        );
        assert!(result.is_err(), "Should fail for non-existent object");
    }

    #[test]
    fn test_s3_backend_missing_content_length() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let endpoint = format!("http://127.0.0.1:{}", port);

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
                // Respond without Content-Length
                let response = "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n";
                let _ = writer.write_all(response.as_bytes());
            }
        });

        thread::sleep(std::time::Duration::from_millis(20));

        // SAFETY: Tests run serially for S3 tests; no other thread reads these env vars concurrently.
        unsafe {
            std::env::set_var("AWS_ACCESS_KEY_ID", "testing");
            std::env::set_var("AWS_SECRET_ACCESS_KEY", "testing");
        }

        let result = S3Backend::new(
            "test-bucket".to_string(),
            "test-key".to_string(),
            "us-east-1".to_string(),
            Some(endpoint),
        );
        assert!(result.is_err(), "Should fail without Content-Length");
    }

    #[test]
    fn test_s3_backend_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<S3Backend>();
        assert_sync::<S3Backend>();
    }
}
