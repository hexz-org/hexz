// Test data generators
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::io::Write;
use tempfile::NamedTempFile;

/// Pattern for structured data generation
#[derive(Debug, Clone, Copy)]
pub enum DataPattern {
    /// All zeros
    Zeros,
    /// All ones (0xFF)
    Ones,
    /// Alternating bytes (0xAA, 0x55, ...)
    Alternating,
    /// Sequential bytes (0x00, 0x01, 0x02, ...)
    Sequential,
    /// Repeating pattern
    Repeating(u8),
}

/// Create a temporary file with specified size and pattern
pub fn create_test_file(size: usize, pattern: DataPattern) -> std::io::Result<NamedTempFile> {
    let mut file = NamedTempFile::new()?;
    let data = match pattern {
        DataPattern::Zeros => vec![0u8; size],
        DataPattern::Ones => vec![0xFFu8; size],
        DataPattern::Alternating => (0..size)
            .map(|i| if i % 2 == 0 { 0xAA } else { 0x55 })
            .collect(),
        DataPattern::Sequential => (0..size).map(|i| (i % 256) as u8).collect(),
        DataPattern::Repeating(byte) => vec![byte; size],
    };
    file.write_all(&data)?;
    file.flush()?;
    Ok(file)
}

/// Create random data with a specific seed for reproducibility
pub fn create_random_data(size: usize) -> Vec<u8> {
    create_random_data_with_seed(size, 42)
}

/// Create random data with a specific seed
pub fn create_random_data_with_seed(size: usize, seed: u64) -> Vec<u8> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut data = vec![0u8; size];
    rng.fill(&mut data[..]);
    data
}

/// Create a test file with specified data
pub fn create_test_file_with_data(data: &[u8]) -> std::io::Result<NamedTempFile> {
    let mut file = NamedTempFile::new()?;
    file.write_all(data)?;
    file.flush()?;
    Ok(file)
}
