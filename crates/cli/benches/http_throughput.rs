//! HTTP backend throughput benchmarks.
//!
//! Measures read throughput of the blocking and async HTTP backends against
//! a minimal in-process HTTP server that supports range requests. Used to
//! compare remote-access performance and validate backend behavior.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use hexz_core::store::StorageBackend;
use hexz_core::store::http::HttpBackend;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;

/// Starts a minimal HTTP server that supports ranged reads over a fixed in-memory buffer.
///
/// **Architectural intent:** Emulates a remote object store or HTTP-based backend with
/// byte-range support so that the `HttpBackend` implementation can be benchmarked in a
/// controlled, in-process environment.
///
/// **Constraints:** Binds to `127.0.0.1` on an ephemeral TCP port and handles each
/// incoming connection in a dedicated thread. The server supports `HEAD` and `GET`
/// requests with a single `/data` path and a `Range` header; it does not implement
/// full HTTP semantics or persistent connections.
///
/// **Side effects:** Spawns background threads that serve requests until the listener
/// is dropped; callers are responsible for joining on the returned handle when the
/// benchmark completes.
fn start_mock_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}/data", addr);

    let data_len = 1024 * 1024;
    let data = vec![b'x'; data_len];
    let data = Arc::new(data);

    #[allow(clippy::manual_strip, clippy::len_zero, clippy::manual_flatten)]
    let handle = thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(mut stream) = stream {
                let data = data.clone();
                thread::spawn(move || {
                    let mut buffer = [0; 4096];
                    if let Ok(n) = stream.read(&mut buffer) {
                        if n == 0 {
                            return;
                        }
                        let request = String::from_utf8_lossy(&buffer[..n]);

                        if request.starts_with("HEAD") {
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n",
                                data.len()
                            );
                            let _ = stream.write_all(response.as_bytes());
                        } else if request.starts_with("GET") {
                            let mut start = 0;
                            let mut end = data.len() - 1;

                            for line in request.lines() {
                                let lower = line.to_lowercase();
                                if lower.starts_with("range:") {
                                    if let Some(val) = lower.split(':').nth(1) {
                                        let val = val.trim();
                                        if val.starts_with("bytes=") {
                                            let range_str = &val[6..];
                                            let parts: Vec<&str> = range_str.split('-').collect();

                                            if parts.len() >= 1 && !parts[0].is_empty() {
                                                if let Ok(s) = parts[0].parse::<usize>() {
                                                    start = s;
                                                }
                                            }

                                            if parts.len() >= 2 && !parts[1].is_empty() {
                                                if let Ok(e) = parts[1].parse::<usize>() {
                                                    end = e;
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if start >= data.len() {
                                start = data.len() - 1;
                            }
                            if end >= data.len() {
                                end = data.len() - 1;
                            }
                            if start > end {
                                start = end;
                            }

                            let len = end - start + 1;
                            let response_header = format!(
                                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
                                len,
                                start,
                                end,
                                data.len()
                            );
                            let _ = stream.write_all(response_header.as_bytes());
                            let _ = stream.write_all(&data[start..=end]);
                            let _ = stream.flush();
                        }
                    }
                });
            }
        }
    });

    (url, handle)
}

/// Benchmarks synchronous (and optionally asynchronous) HTTP reads against the mock backend.
///
/// **Architectural intent:** Measures the sustained throughput of the `HttpBackend`
/// abstraction when reading fixed-size chunks via HTTP range requests, isolating
/// overheads introduced by the client-side storage backend implementation.
///
/// **Constraints:** Assumes the mock server and client run on the same machine and
/// that the network stack is not the primary bottleneck. When the `async-http`
/// feature is enabled, an additional asynchronous benchmark is registered; otherwise
/// only the synchronous variant is exercised.
///
/// **Side effects:** Establishes repeated HTTP connections to the locally spawned
/// server and transfers `chunk_size` bytes per iteration, consuming CPU and network
/// resources during the benchmark run.
fn bench_http_throughput(c: &mut Criterion) {
    let (url, _server) = start_mock_server();
    let chunk_size = 64 * 1024;

    let mut group = c.benchmark_group("HTTP Backend");
    group.throughput(Throughput::Bytes(chunk_size as u64));

    group.bench_function("Sync Read", |b| {
        // Allow restricted IPs for benchmark
        let backend = HttpBackend::new(url.clone(), true).unwrap();
        b.iter(|| {
            let _ = backend.read_exact(0, chunk_size).unwrap();
        })
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .measurement_time(std::time::Duration::from_secs(10));
    targets = bench_http_throughput
}
criterion_main!(benches);
