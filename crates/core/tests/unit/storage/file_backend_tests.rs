//! Unit tests for FileBackend storage

use super::common;
use common::*;

use hexz_core::store::{StorageBackend, local::file::FileBackend};

// Basic operations tests

#[test]
fn test_file_backend_new() {
    let file = create_test_file(1024, DataPattern::Zeros).unwrap();
    let backend = FileBackend::new(file.path()).unwrap();
    assert_eq!(backend.len(), 1024);
    assert!(!backend.is_empty());
}

#[test]
fn test_file_backend_read_full() {
    let data = vec![0x42u8; 4096];
    let file = create_test_file_with_data(&data).unwrap();

    let backend = FileBackend::new(file.path()).unwrap();
    let read_data = backend.read_exact(0, 4096).unwrap();

    assert_eq!(read_data.len(), 4096);
    verify_pattern(&read_data, 0x42);
}

#[test]
fn test_file_backend_read_at_offset() {
    let data = create_random_data(8192);
    let file = create_test_file_with_data(&data).unwrap();

    let backend = FileBackend::new(file.path()).unwrap();
    let read_data = backend.read_exact(1000, 100).unwrap();

    assert_eq!(read_data.len(), 100);
    assert_bytes_equal(&read_data, &data[1000..1100], "offset read");
}

#[test]
fn test_file_backend_read_sequential() {
    let data = create_random_data(10000);
    let file = create_test_file_with_data(&data).unwrap();
    let backend = FileBackend::new(file.path()).unwrap();

    // Read in chunks sequentially
    for offset in (0..10000).step_by(1024) {
        let len = std::cmp::min(1024, 10000 - offset);
        let chunk = backend.read_exact(offset as u64, len).unwrap();
        assert_bytes_equal(&chunk, &data[offset..offset + len], "sequential read");
    }
}

// Boundary condition tests

#[test]
fn test_file_backend_empty_file() {
    let file = create_test_file(0, DataPattern::Zeros).unwrap();
    let backend = FileBackend::new(file.path()).unwrap();

    assert_eq!(backend.len(), 0);
    assert!(backend.is_empty());
}

#[test]
fn test_file_backend_single_byte() {
    let data = vec![0xAAu8];
    let file = create_test_file_with_data(&data).unwrap();
    let backend = FileBackend::new(file.path()).unwrap();

    assert_eq!(backend.len(), 1);
    let read = backend.read_exact(0, 1).unwrap();
    assert_eq!(read[0], 0xAA);
}

#[test]
fn test_file_backend_read_at_start() {
    let data = create_random_data(1000);
    let file = create_test_file_with_data(&data).unwrap();
    let backend = FileBackend::new(file.path()).unwrap();

    let read = backend.read_exact(0, 10).unwrap();
    assert_bytes_equal(&read, &data[0..10], "read at start");
}

#[test]
fn test_file_backend_read_at_end() {
    let data = create_random_data(1000);
    let file = create_test_file_with_data(&data).unwrap();
    let backend = FileBackend::new(file.path()).unwrap();

    let read = backend.read_exact(990, 10).unwrap();
    assert_bytes_equal(&read, &data[990..1000], "read at end");
}

#[test]
fn test_file_backend_read_exact_at_boundary() {
    let data = create_random_data(1024);
    let file = create_test_file_with_data(&data).unwrap();
    let backend = FileBackend::new(file.path()).unwrap();

    // Read exactly to EOF
    let read = backend.read_exact(0, 1024).unwrap();
    assert_eq!(read.len(), 1024);
}

// Error condition tests

#[test]
fn test_file_backend_file_not_found() {
    let result = FileBackend::new(std::path::Path::new("/nonexistent/path/file.dat"));
    assert!(result.is_err());
}

#[test]
fn test_file_backend_read_beyond_eof() {
    let data = vec![0u8; 100];
    let file = create_test_file_with_data(&data).unwrap();
    let backend = FileBackend::new(file.path()).unwrap();

    // Try to read beyond EOF
    let result = backend.read_exact(50, 100);
    assert!(result.is_err());
}

#[test]
fn test_file_backend_read_from_offset_beyond_eof() {
    let data = vec![0u8; 100];
    let file = create_test_file_with_data(&data).unwrap();
    let backend = FileBackend::new(file.path()).unwrap();

    // Start reading beyond EOF
    let result = backend.read_exact(200, 10);
    assert!(result.is_err());
}

// Concurrent read tests

#[test]
fn test_file_backend_concurrent_reads() {
    use std::sync::Arc;
    use std::thread;

    let data = create_random_data(100_000);
    let file = create_test_file_with_data(&data).unwrap();
    let backend = Arc::new(FileBackend::new(file.path()).unwrap());

    let mut handles = vec![];

    // Spawn 10 threads reading from different offsets
    for i in 0..10 {
        let backend = Arc::clone(&backend);
        let data = data.clone();

        let handle = thread::spawn(move || {
            let offset = i * 10000;
            let chunk = backend.read_exact(offset, 1000).unwrap();
            assert_bytes_equal(
                &chunk,
                &data[offset as usize..(offset + 1000) as usize],
                &format!("concurrent read thread {}", i),
            );
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_file_backend_concurrent_overlapping_reads() {
    use std::sync::Arc;
    use std::thread;

    let data = create_random_data(10_000);
    let file = create_test_file_with_data(&data).unwrap();
    let backend = Arc::new(FileBackend::new(file.path()).unwrap());

    let mut handles = vec![];

    // Spawn threads reading overlapping regions
    for i in 0..5 {
        let backend = Arc::clone(&backend);
        let data = data.clone();

        let handle = thread::spawn(move || {
            let offset = i * 1000;
            let chunk = backend.read_exact(offset, 2000).unwrap();
            assert_bytes_equal(
                &chunk,
                &data[offset as usize..(offset + 2000) as usize],
                &format!("overlapping read thread {}", i),
            );
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

// Large file and edge cases

#[test]
fn test_file_backend_large_file() {
    // 10MB file
    let data = create_random_data(10 * 1024 * 1024);
    let file = create_test_file_with_data(&data).unwrap();
    let backend = FileBackend::new(file.path()).unwrap();

    assert_eq!(backend.len() as usize, 10 * 1024 * 1024);

    // Read from various positions
    let chunk1 = backend.read_exact(0, 1024).unwrap();
    assert_bytes_equal(&chunk1, &data[0..1024], "large file start");

    let chunk2 = backend.read_exact(5_000_000, 1024).unwrap();
    assert_bytes_equal(
        &chunk2,
        &data[5_000_000..5_000_000 + 1024],
        "large file middle",
    );
}

#[test]
fn test_file_backend_many_small_reads() {
    let data = create_random_data(10_000);
    let file = create_test_file_with_data(&data).unwrap();
    let backend = FileBackend::new(file.path()).unwrap();

    // Perform 100 small random reads
    for i in 0..100 {
        let offset = (i * 97) % 9900; // Pseudo-random offsets
        let chunk = backend.read_exact(offset as u64, 100).unwrap();
        assert_bytes_equal(
            &chunk,
            &data[offset..offset + 100],
            &format!("small read {}", i),
        );
    }
}

#[test]
fn test_file_backend_different_sizes() {
    // Test with various file sizes
    for size in [0, 1, 10, 100, 1000, 4096, 65536] {
        let data = create_random_data(size);
        let file = create_test_file_with_data(&data).unwrap();
        let backend = FileBackend::new(file.path()).unwrap();

        assert_eq!(backend.len() as usize, size);

        if size > 0 {
            let read = backend.read_exact(0, size).unwrap();
            assert_bytes_equal(&read, &data, &format!("file size {}", size));
        }
    }
}

#[test]
fn test_file_backend_pattern_verification() {
    // Test with different patterns
    for pattern in [
        DataPattern::Zeros,
        DataPattern::Ones,
        DataPattern::Alternating,
        DataPattern::Sequential,
        DataPattern::Repeating(0x42),
    ] {
        let file = create_test_file(1000, pattern).unwrap();
        let backend = FileBackend::new(file.path()).unwrap();

        let data = backend.read_exact(0, 1000).unwrap();
        assert_eq!(data.len(), 1000);

        match pattern {
            DataPattern::Zeros => assert!(is_all_zeros(&data)),
            DataPattern::Ones => assert!(is_all_ones(&data)),
            DataPattern::Sequential => verify_sequential(&data),
            DataPattern::Repeating(b) => verify_pattern(&data, b),
            _ => {} // Skip verification for other patterns
        }
    }
}
