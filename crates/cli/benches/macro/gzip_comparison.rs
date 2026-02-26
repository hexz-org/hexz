//! Gzip vs. Hexz compression comparison benchmarks.
//!
//! Compares compression ratio and encode/decode throughput of gzip against
//! Hexz (LZ4) on the same input data. Uses shared helpers to build
//! large, compressible input files for both formats.

use criterion::{Criterion, criterion_group, criterion_main};
use flate2::Compression;
use flate2::write::GzEncoder;
use hexz_cli::cmd::data::pack;
use hexz_core::File;
use hexz_core::algo::compression::lz4::Lz4Compressor;
use hexz_core::api::file::SnapshotStream;
use hexz_store::local::FileBackend;
use std::fs::File as FsFile;
use std::io::Read;
use std::sync::Arc;
use tempfile::NamedTempFile;

/// Shared helpers for constructing large input files used across macro benchmarks.
///
/// **Architectural intent:** Provides a single, reusable source of compressible
/// test data so that gzip and Hexz comparisons run over identical byte sequences.
///
/// **Constraints:** Included via a relative path to `../common.rs`; directory layout
/// changes must preserve this relationship or update the attribute accordingly.
///
/// **Side effects:** Helper functions in this module write large temporary files to
/// disk as part of benchmark setup.
#[path = "../common.rs"]
mod common;

/// In-memory description of the artifacts produced for gzip vs Hexz comparison.
///
/// **Architectural intent:** Holds references to the generated input, Hexz snapshot,
/// and gzip-compressed file so that benchmarks can seamlessly switch between them
/// without re-running costly setup work.
///
/// **Constraints:** The `_input` field is kept solely to maintain the lifetime of the
/// underlying temporary file; dropping it early would invalidate paths for subsequent
/// reads. The `file_size` is advisory and must match the size used during setup.
///
/// **Side effects:** Instances encapsulate open file descriptors managed by
/// `NamedTempFile`, which are cleaned up when the struct is dropped.
struct BenchSetup {
    _input: NamedTempFile,
    snap: NamedTempFile,
    gzip: NamedTempFile,
    #[allow(dead_code)]
    file_size: usize,
}

/// Builds Hexz and gzip artifacts for a given input size to support head-to-head benchmarks.
///
/// **Architectural intent:** Generates a single large input file, encodes it once into
/// a Hexz snapshot and once into a gzip stream, and returns handles so benchmarks can
/// probe the relative performance characteristics of each format.
///
/// **Constraints:** The function assumes that the `create` command and gzip encoder
/// succeed; failures result in panics via `unwrap()`. The compression level for gzip
/// is fixed to the default and may not correspond to the same effective compression
/// ratio as the Hexz configuration.
///
/// **Side effects:** Performs substantial sequential reads and writes during initial
/// encoding, and creates two temporary files whose lifetimes are tied to the returned
/// `BenchSetup`.
fn setup_comparison(size: usize) -> BenchSetup {
    let input_file = NamedTempFile::new().unwrap();
    let snap_file = NamedTempFile::new().unwrap();
    let gzip_file = NamedTempFile::new().unwrap();

    common::write_large_file(input_file.as_file(), size);

    pack::run(
        Some(input_file.path().to_path_buf()),
        None,
        snap_file.path().to_path_buf(),
        "lz4".to_string(),
        false,
        false, // train_dict
        65536, // block_size
        None,  // min_chunk (auto)
        None,  // avg_chunk (auto)
        None,  // max_chunk (auto)
        None,  // workers
        true,  // silent
    )
    .unwrap();

    let mut input_reader = FsFile::open(input_file.path()).unwrap();
    let mut gz_encoder = GzEncoder::new(
        FsFile::create(gzip_file.path()).unwrap(),
        Compression::default(),
    );
    std::io::copy(&mut input_reader, &mut gz_encoder).unwrap();
    gz_encoder.finish().unwrap();

    BenchSetup {
        _input: input_file,
        snap: snap_file,
        gzip: gzip_file,
        file_size: size,
    }
}

/// Benchmarks the cost of reading the last page of data from Hexz vs gzip.
///
/// **Architectural intent:** Models a page-fault scenario where only the tail of a
/// large disk image is accessed, comparing how efficiently each format can service a
/// small read near the end of the stream.
///
/// **Constraints:** Uses a fixed 100 MiB input and a 4 KiB page size; the offset is
/// computed as `size - 4096`, so changing those constants alters which on-disk layout
/// region is exercised. The gzip path must decode and discard all preceding bytes to
/// reach the target offset, which is representative of traditional stream compressors.
///
/// **Side effects:** Creates and reads large temporary files, repeatedly decompressing
/// data during the benchmark and consuming CPU and I/O resources.
fn bench_page_fault(c: &mut Criterion) {
    let mut group = c.benchmark_group("vm_page_fault");
    group.sample_size(10);

    let size = 100 * 1024 * 1024;
    let setup = setup_comparison(size);

    let offset = (size - 4096) as u64;
    let page_size = 4096;

    group.bench_function("hexz_read_last_page", |b| {
        b.iter(|| {
            let backend = Arc::new(FileBackend::new(setup.snap.path()).unwrap());
            let compressor = Box::new(Lz4Compressor::new());
            let snap = File::new(backend, compressor, None).unwrap();

            let _ = snap
                .read_at(SnapshotStream::Primary, offset, page_size)
                .unwrap();
        })
    });

    group.bench_function("gzip_read_last_page", |b| {
        b.iter(|| {
            let file = FsFile::open(setup.gzip.path()).unwrap();
            let mut decoder = flate2::read::GzDecoder::new(file);

            let mut sink = std::io::sink();

            std::io::copy(&mut Read::by_ref(&mut decoder).take(offset), &mut sink).unwrap();

            let mut buf = vec![0u8; page_size];
            decoder.read_exact(&mut buf).unwrap();
        })
    });

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .measurement_time(std::time::Duration::from_secs(10));
    targets = bench_page_fault
}
criterion_main!(benches);
