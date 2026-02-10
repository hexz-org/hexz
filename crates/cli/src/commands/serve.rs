//! HTTP server for exposing Strata snapshots over range requests.
//!
//! Opens a `.st` file, builds an Axum-based HTTP server that serves disk
//! and memory streams via range-capable endpoints, and optionally daemonizes
//! the process. Used for remote access and benchmarking the HTTP backend.

use anyhow::Result;
use daemonize::Daemonize;
use std::fs::File;
use std::sync::Arc;
use strata_common::constants::DEFAULT_ZSTD_LEVEL;
use strata_core::StrataFile;
use strata_core::algo::compression::{Compressor, lz4::Lz4Compressor, zstd::ZstdCompressor};
use strata_core::format::header::CompressionType;
use strata_core::format::magic::HEADER_SIZE;
use strata_core::store::StorageBackend;
use strata_core::store::local::FileBackend;

/// Serves a Strata snapshot over HTTP, NBD, or as an S3 gateway.
///
/// **Architectural intent:** Spins up the requested server type (HTTP, NBD, or S3)
/// enabling remote consumers to read snapshots without mounting them locally.
///
/// **Constraints:** The `strata_path` must reference a valid `.st` file, and
/// the chosen `port` must be available for binding. When `daemon` is true,
/// the process detaches and logs are redirected to files under `/tmp`.
///
/// **Side effects:** Builds a Tokio runtime, may daemonize the process,
/// performs filesystem I/O to open the snapshot, and listens for and serves
/// incoming requests until shutdown.
pub fn run(strata_path: String, port: u16, daemon: bool, nbd: bool, s3: bool) -> Result<()> {
    if daemon {
        let stdout = File::create("/tmp/strata-serve.log")
            .unwrap_or_else(|_| File::create("/dev/null").unwrap());
        let stderr = File::create("/tmp/strata-serve.err")
            .unwrap_or_else(|_| File::create("/dev/null").unwrap());

        Daemonize::new()
            .working_directory(".")
            .stdout(stdout)
            .stderr(stderr)
            .start()?;
    } else {
        println!("Starting Strata server on port {}", port);
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let backend = Arc::new(FileBackend::new(std::path::Path::new(&strata_path))?);

            let header_bytes = backend.read_exact(0, HEADER_SIZE)?;
            let header: strata_core::format::header::StrataHeader =
                bincode::deserialize(&header_bytes)?;

            let dictionary = if let (Some(offset), Some(length)) =
                (header.dictionary_offset, header.dictionary_length)
            {
                Some(backend.read_exact(offset, length as usize)?.to_vec())
            } else {
                None
            };

            let compressor: Box<dyn Compressor> = match header.compression {
                CompressionType::Lz4 => Box::new(Lz4Compressor::new()),
                CompressionType::Zstd => {
                    Box::new(ZstdCompressor::new(DEFAULT_ZSTD_LEVEL, dictionary))
                }
            };

            let snap = Arc::new(StrataFile::new(backend, compressor, None)?);

            if nbd {
                strata_server::serve_nbd(snap, port).await
            } else if s3 {
                eprintln!("Error: S3 gateway feature is not yet implemented.");
                Ok(())
            } else {
                strata_server::serve_http(snap, port).await
            }
        })
}
