//! Integration tests for the HTTP server.
//!
//! These tests spin up a real HTTP server on a random port and use `reqwest`
//! to make actual HTTP requests, verifying end-to-end behavior including:
//!
//! - Range request handling (bounded, unbounded, no range header)
//! - Response headers (Content-Range, Content-Type, Accept-Ranges)
//! - Error responses (416 Range Not Satisfiable, 404 Not Found)
//! - DoS protection (MAX_CHUNK_SIZE clamping)
//! - Concurrent client access
//! - Data integrity across block boundaries

use hexz_core::Archive;
use hexz_ops::pack::{PackConfig, pack_archive};
use hexz_store::local::FileBackend;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::net::TcpListener;

/// Maximum chunk size the server will return per request (32 MiB).
const MAX_CHUNK_SIZE: u64 = 32 * 1024 * 1024;

/// Test fixture that creates a archive with disk data only.
struct DiskOnlyFixture {
    _temp_dir: TempDir,
    disk_data: Vec<u8>,
    snap: Arc<Archive>,
}

impl DiskOnlyFixture {
    fn new(size: usize, pattern: impl Fn(usize) -> u8) -> Self {
        let temp_dir = TempDir::new().unwrap();
        let disk_data: Vec<u8> = (0..size).map(&pattern).collect();
        let disk_path = temp_dir.path().join("disk.img");
        fs::write(&disk_path, &disk_data).unwrap();

        let snap_path = temp_dir.path().join("archive.hxz");
        let config = PackConfig {
            input: disk_path,
            output: snap_path.clone(),
            compression: "lz4".to_string(),
            encrypt: false,
            password: None,
            train_dict: false,
            block_size: 65536,
            min_chunk: Some(16384),
            avg_chunk: Some(65536),
            max_chunk: Some(131072),
            ..Default::default()
        };
        pack_archive(config, None::<fn(u64, u64)>).unwrap();

        let backend = Arc::new(FileBackend::new(&snap_path).unwrap());
        let snap = Archive::open(backend, None).unwrap();

        Self {
            _temp_dir: temp_dir,
            disk_data,
            snap,
        }
    }
}

/// Test fixture that creates a archive with both disk and memory data.
struct DualStreamFixture {
    _temp_dir: TempDir,
    disk_data: Vec<u8>,
    mem_data: Vec<u8>,
    snap: Arc<Archive>,
}

impl DualStreamFixture {
    fn new(main_size: usize, mem_size: usize) -> Self {
        let temp_dir = TempDir::new().unwrap();
        let input_dir = temp_dir.path().join("input");
        fs::create_dir(&input_dir).unwrap();

        let disk_data: Vec<u8> = (0..main_size).map(|i| (i % 256) as u8).collect();
        let mem_data: Vec<u8> = vec![0xBB; mem_size];

        let disk_path = input_dir.join("disk");
        let mem_path = input_dir.join("memory");
        fs::write(&disk_path, &disk_data).unwrap();
        fs::write(&mem_path, &mem_data).unwrap();

        let snap_path = temp_dir.path().join("archive.hxz");
        let config = PackConfig {
            input: input_dir,
            output: snap_path.clone(),
            compression: "lz4".to_string(),
            encrypt: false,
            password: None,
            train_dict: false,
            block_size: 65536,
            min_chunk: Some(16384),
            avg_chunk: Some(65536),
            max_chunk: Some(131072),
            ..Default::default()
        };
        pack_archive(config, None::<fn(u64, u64)>).unwrap();

        let backend = Arc::new(FileBackend::new(&snap_path).unwrap());
        let snap = Archive::open(backend, None).unwrap();

        Self {
            _temp_dir: temp_dir,
            disk_data,
            mem_data,
            snap,
        }
    }
}

/// Start the HTTP server in a background Tokio task.
///
/// Binds to port 0 (OS-assigned) and passes the listener directly to
/// `serve_http_with_listener`, avoiding the TOCTOU race of discovering a
/// free port and re-binding by number. Returns the assigned port.
async fn start_server(snap: Arc<Archive>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        hexz_server::serve_http_with_listener(snap, listener)
            .await
            .unwrap();
    });

    // Wait for the server to be ready by polling the port.
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .is_ok()
        {
            return port;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("Server did not start within 1 second");
}

fn base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

// ───────────────────────────────────────────────────────────────────────────
// Basic Endpoint Tests
// ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_get_disk_no_range_header() {
    let fixture = DiskOnlyFixture::new(256 * 1024, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .send()
        .await
        .unwrap();

    // Without a Range header, server returns 206 with data clamped to MAX_CHUNK_SIZE
    // or the full stream if smaller.
    assert!(resp.status().is_success());

    let content_range = resp
        .headers()
        .get("content-range")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(content_range.starts_with("bytes 0-"));

    let body = resp.bytes().await.unwrap();
    let expected_len = fixture.disk_data.len().min(MAX_CHUNK_SIZE as usize);
    assert_eq!(body.len(), expected_len);
    assert_eq!(&body[..], &fixture.disk_data[..expected_len]);
}

#[tokio::test]
async fn test_get_memory_no_range_header() {
    let fixture = DualStreamFixture::new(128 * 1024, 64 * 1024);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/memory", base_url(port)))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());

    let body = resp.bytes().await.unwrap();
    let expected_len = fixture.mem_data.len().min(MAX_CHUNK_SIZE as usize);
    assert_eq!(body.len(), expected_len);
    assert_eq!(&body[..], &fixture.mem_data[..expected_len]);
}

#[tokio::test]
async fn test_nonexistent_endpoint_returns_404() {
    let fixture = DiskOnlyFixture::new(64 * 1024, |_| 0x42);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/nonexistent", base_url(port)))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
}

// ───────────────────────────────────────────────────────────────────────────
// Range Request Tests
// ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_bounded_range_request() {
    let fixture = DiskOnlyFixture::new(256 * 1024, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", "bytes=0-4095")
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());

    let content_range = resp
        .headers()
        .get("content-range")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let total = fixture.disk_data.len();
    assert_eq!(content_range, format!("bytes 0-4095/{total}"));

    let accept_ranges = resp
        .headers()
        .get("accept-ranges")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(accept_ranges, "bytes");

    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(content_type, "application/octet-stream");

    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), 4096);
    assert_eq!(&body[..], &fixture.disk_data[0..4096]);
}

#[tokio::test]
async fn test_bounded_range_mid_stream() {
    let fixture = DiskOnlyFixture::new(256 * 1024, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let start = 100_000usize;
    let end = 104_095usize;

    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", format!("bytes={start}-{end}"))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());

    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), end - start + 1);
    assert_eq!(&body[..], &fixture.disk_data[start..=end]);
}

#[tokio::test]
async fn test_unbounded_range_request() {
    let fixture = DiskOnlyFixture::new(256 * 1024, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let start = 1024usize;

    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", format!("bytes={start}-"))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());

    let body = resp.bytes().await.unwrap();
    let expected_end = fixture.disk_data.len() - 1;
    let expected_len = expected_end - start + 1;
    assert_eq!(body.len(), expected_len);
    assert_eq!(&body[..], &fixture.disk_data[start..]);
}

#[tokio::test]
async fn test_single_byte_range() {
    let fixture = DiskOnlyFixture::new(64 * 1024, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", "bytes=0-0")
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());

    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0], fixture.disk_data[0]);
}

#[tokio::test]
async fn test_range_at_exact_eof() {
    let size = 64 * 1024usize;
    let fixture = DiskOnlyFixture::new(size, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    // Request the last byte
    let last = size - 1;
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", format!("bytes={last}-{last}"))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());

    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0], fixture.disk_data[last]);
}

#[tokio::test]
async fn test_range_last_block() {
    let size = 256 * 1024usize;
    let fixture = DiskOnlyFixture::new(size, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let start = size - 4096;
    let end = size - 1;
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", format!("bytes={start}-{end}"))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());

    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), 4096);
    assert_eq!(&body[..], &fixture.disk_data[start..=end]);
}

// ───────────────────────────────────────────────────────────────────────────
// Error Response Tests
// ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_invalid_range_missing_prefix() {
    let fixture = DiskOnlyFixture::new(64 * 1024, |_| 0x42);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", "0-1023")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 416);
}

#[tokio::test]
async fn test_invalid_range_out_of_bounds() {
    let size = 64 * 1024usize;
    let fixture = DiskOnlyFixture::new(size, |_| 0x42);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    // Request beyond stream size
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", format!("bytes=0-{size}"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 416);
}

#[tokio::test]
async fn test_invalid_range_inverted() {
    let fixture = DiskOnlyFixture::new(64 * 1024, |_| 0x42);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", "bytes=1000-500")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 416);
}

#[tokio::test]
async fn test_invalid_range_suffix_not_supported() {
    let fixture = DiskOnlyFixture::new(64 * 1024, |_| 0x42);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", "bytes=-500")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 416);
}

#[tokio::test]
async fn test_invalid_range_non_numeric() {
    let fixture = DiskOnlyFixture::new(64 * 1024, |_| 0x42);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", "bytes=abc-def")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 416);
}

#[tokio::test]
async fn test_invalid_range_empty_value() {
    let fixture = DiskOnlyFixture::new(64 * 1024, |_| 0x42);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", "bytes=")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 416);
}

#[tokio::test]
async fn test_start_beyond_eof() {
    let size = 64 * 1024usize;
    let fixture = DiskOnlyFixture::new(size, |_| 0x42);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", format!("bytes={}-", size + 1000))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 416);
}

// ───────────────────────────────────────────────────────────────────────────
// DoS Protection / Clamping Tests
// ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_large_range_is_clamped_to_max_chunk_size() {
    // Create a archive larger than MAX_CHUNK_SIZE (32 MiB)
    let size = 48 * 1024 * 1024; // 48 MiB
    let fixture = DiskOnlyFixture::new(size, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    // Request the entire 48 MiB — should be clamped to 32 MiB
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", format!("bytes=0-{}", size - 1))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());

    let content_range = resp
        .headers()
        .get("content-range")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let expected_end = MAX_CHUNK_SIZE - 1;
    assert_eq!(content_range, format!("bytes 0-{expected_end}/{size}"));

    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), MAX_CHUNK_SIZE as usize);
    assert_eq!(&body[..], &fixture.disk_data[..MAX_CHUNK_SIZE as usize]);
}

#[tokio::test]
async fn test_no_range_header_large_file_is_clamped() {
    let size = 48 * 1024 * 1024;
    let fixture = DiskOnlyFixture::new(size, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());

    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), MAX_CHUNK_SIZE as usize);
}

#[tokio::test]
async fn test_clamping_with_nonzero_start() {
    let size = 48 * 1024 * 1024;
    let fixture = DiskOnlyFixture::new(size, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let start = 1024u64;
    // Request from offset 1024 to end — more than MAX_CHUNK_SIZE from start
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", format!("bytes={start}-"))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());

    let expected_end = start + MAX_CHUNK_SIZE - 1;
    let content_range = resp
        .headers()
        .get("content-range")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        content_range,
        format!("bytes {start}-{expected_end}/{size}")
    );

    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), MAX_CHUNK_SIZE as usize);
    assert_eq!(
        &body[..],
        &fixture.disk_data[start as usize..(start + MAX_CHUNK_SIZE) as usize]
    );
}

// ───────────────────────────────────────────────────────────────────────────
// Content-Range Header Format Tests
// ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_content_range_format_bounded() {
    let size = 128 * 1024usize;
    let fixture = DiskOnlyFixture::new(size, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", "bytes=1000-1999")
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());

    let content_range = resp
        .headers()
        .get("content-range")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(content_range, format!("bytes 1000-1999/{size}"));
}

#[tokio::test]
async fn test_response_headers_present() {
    let fixture = DiskOnlyFixture::new(64 * 1024, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", "bytes=0-1023")
        .send()
        .await
        .unwrap();

    assert!(resp.headers().get("content-range").is_some());
    assert!(resp.headers().get("accept-ranges").is_some());
    assert!(resp.headers().get("content-type").is_some());

    assert_eq!(
        resp.headers()
            .get("accept-ranges")
            .unwrap()
            .to_str()
            .unwrap(),
        "bytes"
    );
    assert_eq!(
        resp.headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap(),
        "application/octet-stream"
    );
}

// ───────────────────────────────────────────────────────────────────────────
// Dual-Stream Tests
// ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_disk_and_memory_return_different_data() {
    let fixture = DualStreamFixture::new(128 * 1024, 64 * 1024);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();

    let disk_resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", "bytes=0-1023")
        .send()
        .await
        .unwrap();
    let disk_body = disk_resp.bytes().await.unwrap();

    let mem_resp = client
        .get(format!("{}/memory", base_url(port)))
        .header("Range", "bytes=0-1023")
        .send()
        .await
        .unwrap();
    let mem_body = mem_resp.bytes().await.unwrap();

    // Disk data is sequential, memory data is 0xBB
    assert_eq!(&disk_body[..], &fixture.disk_data[..1024]);
    assert_eq!(&mem_body[..], &fixture.mem_data[..1024]);
    assert_ne!(&disk_body[..], &mem_body[..]);
}

#[tokio::test]
async fn test_memory_stream_full_read() {
    let mem_size = 64 * 1024;
    let fixture = DualStreamFixture::new(128 * 1024, mem_size);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/memory", base_url(port)))
        .header("Range", format!("bytes=0-{}", mem_size - 1))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());
    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), mem_size);
    assert_eq!(&body[..], &fixture.mem_data[..]);
}

// ───────────────────────────────────────────────────────────────────────────
// Cross-Block Boundary Tests
// ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_read_spanning_block_boundary() {
    // Block size is 65536 bytes. Read across the boundary.
    let size = 256 * 1024usize;
    let fixture = DiskOnlyFixture::new(size, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    // Read 8KB straddling the 64KB block boundary
    let start = 65536 - 4096;
    let end = 65536 + 4095;
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", format!("bytes={start}-{end}"))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());
    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), 8192);
    assert_eq!(&body[..], &fixture.disk_data[start..=end]);
}

#[tokio::test]
async fn test_read_spanning_multiple_blocks() {
    let size = 512 * 1024usize;
    let fixture = DiskOnlyFixture::new(size, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    // Read 192KB starting at 32KB — spans blocks 0, 1, 2, 3
    let start = 32768usize;
    let len = 192 * 1024;
    let end = start + len - 1;
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", format!("bytes={start}-{end}"))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());
    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), len);
    assert_eq!(&body[..], &fixture.disk_data[start..=end]);
}

// ───────────────────────────────────────────────────────────────────────────
// Concurrent Access Tests
// ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_concurrent_reads_same_endpoint() {
    let size = 256 * 1024usize;
    let fixture = DiskOnlyFixture::new(size, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let disk_data = fixture.disk_data.clone();

    let mut handles = Vec::new();
    for i in 0..20 {
        let client = client.clone();
        let url = format!("{}/disk", base_url(port));
        let expected = disk_data.clone();

        handles.push(tokio::spawn(async move {
            let start = (i * 4096) % size;
            let end = start + 4095;
            let resp = client
                .get(&url)
                .header("Range", format!("bytes={start}-{end}"))
                .send()
                .await
                .unwrap();

            assert!(resp.status().is_success(), "Request {i} failed");
            let body = resp.bytes().await.unwrap();
            assert_eq!(body.len(), 4096, "Request {i}: wrong body length");
            assert_eq!(
                &body[..],
                &expected[start..=end],
                "Request {i}: data mismatch at offset {start}"
            );
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_concurrent_reads_different_endpoints() {
    let fixture = DualStreamFixture::new(128 * 1024, 64 * 1024);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let disk_data = fixture.disk_data.clone();
    let mem_data = fixture.mem_data.clone();

    let mut handles = Vec::new();

    // 10 disk readers
    for i in 0..10 {
        let client = client.clone();
        let url = format!("{}/disk", base_url(port));
        let expected = disk_data.clone();
        handles.push(tokio::spawn(async move {
            let resp = client
                .get(&url)
                .header("Range", "bytes=0-4095")
                .send()
                .await
                .unwrap();
            assert!(resp.status().is_success(), "Disk request {i} failed");
            let body = resp.bytes().await.unwrap();
            assert_eq!(&body[..], &expected[..4096]);
        }));
    }

    // 10 memory readers
    for i in 0..10 {
        let client = client.clone();
        let url = format!("{}/memory", base_url(port));
        let expected = mem_data.clone();
        handles.push(tokio::spawn(async move {
            let resp = client
                .get(&url)
                .header("Range", "bytes=0-4095")
                .send()
                .await
                .unwrap();
            assert!(resp.status().is_success(), "Memory request {i} failed");
            let body = resp.bytes().await.unwrap();
            assert_eq!(&body[..], &expected[..4096]);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Sequential Multi-Request (Chunked Read) Tests
// ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_sequential_chunked_read_covers_full_stream() {
    let size = 256 * 1024usize;
    let fixture = DiskOnlyFixture::new(size, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let chunk_size = 16 * 1024usize;
    let mut reassembled = Vec::with_capacity(size);
    let mut offset = 0usize;

    while offset < size {
        let end = (offset + chunk_size - 1).min(size - 1);
        let resp = client
            .get(format!("{}/disk", base_url(port)))
            .header("Range", format!("bytes={offset}-{end}"))
            .send()
            .await
            .unwrap();

        assert!(resp.status().is_success());
        let body = resp.bytes().await.unwrap();
        reassembled.extend_from_slice(&body);
        offset = end + 1;
    }

    assert_eq!(reassembled.len(), size);
    assert_eq!(&reassembled[..], &fixture.disk_data[..]);
}

// ───────────────────────────────────────────────────────────────────────────
// Data Integrity Tests
// ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_data_integrity_entire_disk_stream() {
    let size = 128 * 1024usize;
    let fixture = DiskOnlyFixture::new(size, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", format!("bytes=0-{}", size - 1))
        .send()
        .await
        .unwrap();

    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), size);

    // Byte-by-byte verification at key offsets
    for offset in [0, 1, 255, 256, 65535, 65536, size - 1] {
        assert_eq!(
            body[offset], fixture.disk_data[offset],
            "Mismatch at offset {offset}"
        );
    }

    // Full comparison
    assert_eq!(&body[..], &fixture.disk_data[..]);
}

#[tokio::test]
async fn test_data_integrity_random_offsets() {
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    let size = 512 * 1024usize;
    let fixture = DiskOnlyFixture::new(size, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let mut rng = StdRng::seed_from_u64(42);

    for _ in 0..15 {
        let start: usize = rng.gen_range(0..size - 1);
        let max_len = (size - start).min(64 * 1024);
        let len: usize = rng.gen_range(1..=max_len);
        let end = start + len - 1;

        let resp = client
            .get(format!("{}/disk", base_url(port)))
            .header("Range", format!("bytes={start}-{end}"))
            .send()
            .await
            .unwrap();

        assert!(resp.status().is_success());
        let body = resp.bytes().await.unwrap();
        assert_eq!(body.len(), len, "Wrong length for range {start}-{end}");
        assert_eq!(
            &body[..],
            &fixture.disk_data[start..=end],
            "Data mismatch for range {start}-{end}"
        );
    }
}

// ───────────────────────────────────────────────────────────────────────────
// HTTP Method Tests
// ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_post_method_not_allowed() {
    let fixture = DiskOnlyFixture::new(64 * 1024, |_| 0x42);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/disk", base_url(port)))
        .send()
        .await
        .unwrap();

    // Axum returns 405 Method Not Allowed for unsupported methods on existing routes
    assert_eq!(resp.status(), 405);
}

#[tokio::test]
async fn test_put_method_not_allowed() {
    let fixture = DiskOnlyFixture::new(64 * 1024, |_| 0x42);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .put(format!("{}/disk", base_url(port)))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 405);
}

#[tokio::test]
async fn test_delete_method_not_allowed() {
    let fixture = DiskOnlyFixture::new(64 * 1024, |_| 0x42);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .delete(format!("{}/disk", base_url(port)))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 405);
}

// ───────────────────────────────────────────────────────────────────────────
// Repeated Reads (Cache Behavior) Tests
// ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_repeated_reads_return_same_data() {
    let fixture = DiskOnlyFixture::new(128 * 1024, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();

    let mut first_body: Option<bytes::Bytes> = None;
    for i in 0..5 {
        let resp = client
            .get(format!("{}/disk", base_url(port)))
            .header("Range", "bytes=0-8191")
            .send()
            .await
            .unwrap();

        assert!(resp.status().is_success());
        let body = resp.bytes().await.unwrap();
        assert_eq!(body.len(), 8192);

        if let Some(ref first) = first_body {
            assert_eq!(&body[..], &first[..], "Mismatch on read {i}");
        } else {
            first_body = Some(body);
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Edge Case Tests
// ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_exact_block_size_read() {
    // Read exactly one block (65536 bytes)
    let size = 256 * 1024usize;
    let fixture = DiskOnlyFixture::new(size, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", "bytes=0-65535")
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());
    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), 65536);
    assert_eq!(&body[..], &fixture.disk_data[..65536]);
}

#[tokio::test]
async fn test_block_aligned_start() {
    let size = 256 * 1024usize;
    let fixture = DiskOnlyFixture::new(size, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    // Start at block boundary, read less than one block
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", "bytes=65536-66559")
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());
    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), 1024);
    assert_eq!(&body[..], &fixture.disk_data[65536..66560]);
}

#[tokio::test]
async fn test_small_archive() {
    // Archive smaller than one block
    let size = 1024usize;
    let fixture = DiskOnlyFixture::new(size, |i| (i % 256) as u8);
    let port = start_server(fixture.snap.clone()).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/disk", base_url(port)))
        .header("Range", format!("bytes=0-{}", size - 1))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());
    let body = resp.bytes().await.unwrap();
    assert_eq!(body.len(), size);
    assert_eq!(&body[..], &fixture.disk_data[..]);
}
