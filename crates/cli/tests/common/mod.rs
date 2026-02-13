//! Common test utilities shared across integration tests

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Test environment with auto-cleanup
pub struct TestEnv {
    pub temp_dir: TempDir,
    pub snapshot_path: PathBuf,
    pub output_path: PathBuf,
}

impl TestEnv {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let snapshot_path = temp_dir.path().join("test.strata");
        let output_path = temp_dir.path().join("output");
        fs::create_dir_all(&output_path).expect("Failed to create output dir");

        Self {
            temp_dir,
            snapshot_path,
            output_path,
        }
    }

    pub fn path(&self) -> &Path {
        self.temp_dir.path()
    }

    pub fn create_test_file(&self, name: &str, size: usize) -> PathBuf {
        let path = self.temp_dir.path().join(name);
        let data = vec![0xAB; size];
        fs::write(&path, data).expect("Failed to write test file");
        path
    }

    pub fn create_pattern_file(&self, name: &str, pattern: &[u8], repeat: usize) -> PathBuf {
        let path = self.temp_dir.path().join(name);
        let mut data = Vec::new();
        for _ in 0..repeat {
            data.extend_from_slice(pattern);
        }
        fs::write(&path, data).expect("Failed to write pattern file");
        path
    }
}

/// Generate random test data
pub fn random_data(size: usize) -> Vec<u8> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..size).map(|_| rng.r#gen()).collect()
}

/// Generate compressible test data (repeating patterns)
pub fn compressible_data(size: usize) -> Vec<u8> {
    let pattern = b"The quick brown fox jumps over the lazy dog. ";
    let pattern_len = pattern.len();
    let mut data = Vec::with_capacity(size);
    for i in 0..size {
        data.push(pattern[i % pattern_len]);
    }
    data
}

/// Verify two byte slices are equal
pub fn assert_bytes_eq(a: &[u8], b: &[u8]) {
    assert_eq!(a.len(), b.len(), "Length mismatch");
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        assert_eq!(x, y, "Byte mismatch at offset {}", i);
    }
}
