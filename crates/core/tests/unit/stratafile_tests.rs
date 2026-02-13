//! Unit tests for StrataFile API to improve coverage from 70% to 80%+.
//!
//! Tests focus on edge cases, error handling, and less-common code paths.

use std::io::Write;
use std::sync::Arc;
use strata_core::algo::compression::lz4::Lz4Compressor;
use strata_core::store::local::file::FileBackend;
use strata_core::{SnapshotStream, StrataFile};
use tempfile::NamedTempFile;

use crate::common::*;

/// Test reading at various offsets including edge cases
#[test]
fn test_read_at_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
    let (snap_path, original_data) = create_simple_snapshot()?;

    let backend = Arc::new(FileBackend::new(&snap_path)?);
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None)?;

    // Test reading at offset 0
    let data = snapshot.read_at(SnapshotStream::Disk, 0, 100)?;
    assert_eq!(data.len(), 100);
    assert_eq!(&data[..], &original_data[0..100]);

    // Test reading at max offset
    let size = snapshot.size(SnapshotStream::Disk);
    let data = snapshot.read_at(SnapshotStream::Disk, size - 10, 10)?;
    assert_eq!(data.len(), 10);

    // Test reading beyond size returns empty
    let data = snapshot.read_at(SnapshotStream::Disk, size + 1000, 100)?;
    assert!(data.is_empty());

    // Test reading that extends beyond size (partial read)
    let data = snapshot.read_at(SnapshotStream::Disk, size - 5, 100)?;
    assert_eq!(data.len(), 5);

    Ok(())
}

/// Test read_at_into with buffer filling and zero-padding
#[test]
fn test_read_at_into_buffer_handling() -> Result<(), Box<dyn std::error::Error>> {
    let (snap_path, original_data) = create_simple_snapshot()?;

    let backend = Arc::new(FileBackend::new(&snap_path)?);
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None)?;

    // Test normal read into buffer
    let mut buffer = vec![0xFF; 100];
    snapshot.read_at_into(SnapshotStream::Disk, 0, &mut buffer)?;
    assert_eq!(&buffer[..], &original_data[0..100]);

    // Test reading beyond file size (should zero-fill)
    let size = snapshot.size(SnapshotStream::Disk);
    let mut buffer = vec![0xFF; 100];
    snapshot.read_at_into(SnapshotStream::Disk, size + 10, &mut buffer)?;
    assert_eq!(buffer, vec![0; 100]);

    // Test partial read with zero-fill
    let mut buffer = vec![0xFF; 100];
    snapshot.read_at_into(SnapshotStream::Disk, size - 10, &mut buffer)?;
    assert_eq!(&buffer[0..10], &original_data[original_data.len() - 10..]);
    assert_eq!(&buffer[10..], &vec![0; 90][..]);

    // Test zero-length buffer
    let mut buffer = vec![];
    snapshot.read_at_into(SnapshotStream::Disk, 0, &mut buffer)?;
    assert_eq!(buffer.len(), 0);

    Ok(())
}

/// Test memory stream access
#[test]
fn test_memory_stream_access() -> Result<(), Box<dyn std::error::Error>> {
    let (snap_path, _) = create_snapshot_with_memory()?;

    let backend = Arc::new(FileBackend::new(&snap_path)?);
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None)?;

    let mem_size = snapshot.size(SnapshotStream::Memory);
    assert!(mem_size > 0, "Memory stream should have data");

    // Read from memory stream
    let data = snapshot.read_at(SnapshotStream::Memory, 0, 100)?;
    assert_eq!(data.len(), 100);

    // Test reading beyond memory size
    let data = snapshot.read_at(SnapshotStream::Memory, mem_size + 100, 10)?;
    assert!(data.is_empty());

    Ok(())
}

/// Test custom cache capacity settings
#[test]
fn test_custom_cache_capacity() -> Result<(), Box<dyn std::error::Error>> {
    let (snap_path, _) = create_simple_snapshot()?;

    let backend = Arc::new(FileBackend::new(&snap_path)?);
    let compressor = Box::new(Lz4Compressor::new());

    // Test with small cache
    let snapshot = StrataFile::with_cache(
        backend.clone(),
        compressor,
        None,
        Some(4096), // 4KB cache
        None,
    )?;

    let data = snapshot.read_at(SnapshotStream::Disk, 0, 100)?;
    assert_eq!(data.len(), 100);

    // Test with large cache
    let compressor2 = Box::new(Lz4Compressor::new());
    let snapshot2 = StrataFile::with_cache(
        backend,
        compressor2,
        None,
        Some(100_000_000), // 100MB cache
        None,
    )?;

    let data2 = snapshot2.read_at(SnapshotStream::Disk, 0, 100)?;
    assert_eq!(data2.len(), 100);

    Ok(())
}

/// Test prefetching configuration
#[test]
fn test_prefetch_configuration() -> Result<(), Box<dyn std::error::Error>> {
    let (snap_path, _) = create_simple_snapshot()?;

    let backend = Arc::new(FileBackend::new(&snap_path)?);
    let compressor = Box::new(Lz4Compressor::new());

    // Test with prefetching enabled
    let snapshot = StrataFile::with_cache(
        backend,
        compressor,
        None,
        None,
        Some(4), // Prefetch 4 blocks ahead
    )?;

    // Do a read to trigger prefetching
    let data = snapshot.read_at(SnapshotStream::Disk, 0, 1024)?;
    assert_eq!(data.len(), 1024);

    // Small sleep to allow prefetch thread to start
    std::thread::sleep(std::time::Duration::from_millis(10));

    Ok(())
}

/// Test reading empty/sparse regions
#[test]
fn test_sparse_region_handling() -> Result<(), Box<dyn std::error::Error>> {
    let (snap_path, _) = create_simple_snapshot()?;

    let backend = Arc::new(FileBackend::new(&snap_path)?);
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None)?;

    // If the snapshot has no pages (empty), reads should return zeros
    // This tests the empty pages handling in read_at_into_uninit

    let size = snapshot.size(SnapshotStream::Disk);
    if size > 0 {
        let data = snapshot.read_at(SnapshotStream::Disk, 0, size as usize)?;
        assert_eq!(data.len(), size as usize);
    }

    Ok(())
}

/// Test size() method for both streams
#[test]
fn test_size_queries() -> Result<(), Box<dyn std::error::Error>> {
    let (snap_path, original_data) = create_simple_snapshot()?;

    let backend = Arc::new(FileBackend::new(&snap_path)?);
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None)?;

    let disk_size = snapshot.size(SnapshotStream::Disk);
    assert_eq!(disk_size as usize, original_data.len());

    let memory_size = snapshot.size(SnapshotStream::Memory);
    // Simple snapshot has no memory, should be 0
    assert_eq!(memory_size, 0);

    Ok(())
}

/// Test parallel decompression path with multiple blocks
#[test]
fn test_parallel_decompression_path() -> Result<(), Box<dyn std::error::Error>> {
    // Create a larger snapshot to trigger parallel decompression
    let (snap_path, _) = create_multi_block_snapshot()?;

    let backend = Arc::new(FileBackend::new(&snap_path)?);
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None)?;

    // Read across multiple blocks (should trigger parallel path)
    let data = snapshot.read_at(SnapshotStream::Disk, 0, 128 * 1024)?;
    assert!(data.len() >= 100_000);

    Ok(())
}

/// Test reading from snapshot without compression
#[test]
fn test_uncompressed_read() -> Result<(), Box<dyn std::error::Error>> {
    let (snap_path, original_data) = create_simple_snapshot()?;

    let backend = Arc::new(FileBackend::new(&snap_path)?);
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None)?;

    // Read entire file
    let size = snapshot.size(SnapshotStream::Disk);
    let data = snapshot.read_at(SnapshotStream::Disk, 0, size as usize)?;

    assert_eq!(data.len(), original_data.len());
    assert_eq!(&data[..100], &original_data[..100]);

    Ok(())
}

/// Test error handling for invalid magic bytes
#[test]
fn test_invalid_magic_bytes_error() -> Result<(), Box<dyn std::error::Error>> {
    let mut temp_file = NamedTempFile::new()?;

    // Write invalid magic bytes
    temp_file.write_all(b"INVALID_MAGIC")?;
    temp_file.write_all(&[0u8; 4096])?; // Pad with zeros
    temp_file.flush()?;

    let backend = Arc::new(FileBackend::new(temp_file.path())?);
    let compressor = Box::new(Lz4Compressor::new());

    let result = StrataFile::new(backend, compressor, None);
    assert!(result.is_err(), "Should fail with invalid magic bytes");

    Ok(())
}

/// Test reading with misaligned offsets and lengths
#[test]
fn test_misaligned_reads() -> Result<(), Box<dyn std::error::Error>> {
    let (snap_path, original_data) = create_simple_snapshot()?;

    let backend = Arc::new(FileBackend::new(&snap_path)?);
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None)?;

    // Read at odd offsets with odd lengths
    let data = snapshot.read_at(SnapshotStream::Disk, 13, 47)?;
    assert_eq!(data.len(), 47);
    assert_eq!(&data[..], &original_data[13..60]);

    // Read spanning block boundaries
    let data = snapshot.read_at(SnapshotStream::Disk, 4090, 20)?;
    assert_eq!(data.len(), 20);

    Ok(())
}

/// Test read_at_into_uninit_bytes (FFI-friendly version)
#[test]
fn test_read_at_into_uninit_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let (snap_path, original_data) = create_simple_snapshot()?;

    let backend = Arc::new(FileBackend::new(&snap_path)?);
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None)?;

    let mut buffer = vec![0xFF; 100];
    snapshot.read_at_into_uninit_bytes(SnapshotStream::Disk, 0, &mut buffer)?;

    assert_eq!(&buffer[..], &original_data[0..100]);

    Ok(())
}

/// Test cache hit/miss behavior
#[test]
fn test_cache_behavior() -> Result<(), Box<dyn std::error::Error>> {
    let (snap_path, _) = create_simple_snapshot()?;

    let backend = Arc::new(FileBackend::new(&snap_path)?);
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None)?;

    // First read (cache miss)
    let data1 = snapshot.read_at(SnapshotStream::Disk, 0, 100)?;

    // Second read (cache hit)
    let data2 = snapshot.read_at(SnapshotStream::Disk, 0, 100)?;

    assert_eq!(data1, data2);

    Ok(())
}

/// Test reading with zero-length request
#[test]
fn test_zero_length_read() -> Result<(), Box<dyn std::error::Error>> {
    let (snap_path, _) = create_simple_snapshot()?;

    let backend = Arc::new(FileBackend::new(&snap_path)?);
    let compressor = Box::new(Lz4Compressor::new());
    let snapshot = StrataFile::new(backend, compressor, None)?;

    let data = snapshot.read_at(SnapshotStream::Disk, 0, 0)?;
    assert_eq!(data.len(), 0);

    let mut buffer = vec![];
    snapshot.read_at_into(SnapshotStream::Disk, 0, &mut buffer)?;
    assert_eq!(buffer.len(), 0);

    Ok(())
}
