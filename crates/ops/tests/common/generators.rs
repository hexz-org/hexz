// Test data generators
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::io::Write;
use std::path::PathBuf;
use tempfile::{NamedTempFile, TempDir};

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

/// Create sparse data (mostly zeros with islands of non-zero data)
/// `sparsity` is the fraction of zeros (0.0 = no zeros, 1.0 = all zeros)
pub fn create_sparse_data(size: usize, sparsity: f64) -> Vec<u8> {
    assert!(
        (0.0..=1.0).contains(&sparsity),
        "Sparsity must be in [0.0, 1.0]"
    );

    let mut rng = StdRng::seed_from_u64(42);
    let mut data = vec![0u8; size];

    // Number of non-zero bytes
    let non_zero_count = ((1.0 - sparsity) * size as f64) as usize;

    // Randomly place non-zero bytes
    for _ in 0..non_zero_count {
        let idx = rng.gen_range(0..size);
        let val: u8 = rng.r#gen();
        data[idx] = if val == 0 { 1 } else { val }; // Ensure non-zero
    }

    data
}

/// Create data with controlled entropy
/// `entropy` is approximate entropy in bits per byte (0.0 = all same, 8.0 = random)
pub fn create_compressible_data(size: usize, entropy: f64) -> Vec<u8> {
    assert!(
        (0.0..=8.0).contains(&entropy),
        "Entropy must be in [0.0, 8.0]"
    );

    if entropy < 0.1 {
        // Very low entropy: all zeros
        return vec![0u8; size];
    } else if entropy > 7.9 {
        // High entropy: random data
        return create_random_data(size);
    }

    // Medium entropy: mix of repeating patterns and random data
    let mut rng = StdRng::seed_from_u64(42);
    let random_ratio = entropy / 8.0;
    let mut data = Vec::with_capacity(size);

    let pattern_size = ((1.0 - random_ratio) * 256.0) as usize + 1;
    let pattern: Vec<u8> = (0..pattern_size).map(|i| (i % 256) as u8).collect();

    let mut pos = 0;
    while data.len() < size {
        let rand_val: f64 = rng.r#gen();
        if rand_val < random_ratio {
            let byte_val: u8 = rng.r#gen();
            data.push(byte_val);
        } else {
            data.push(pattern[pos % pattern.len()]);
            pos += 1;
        }
    }

    data.truncate(size);
    data
}

/// Create structured data with repeating patterns
/// `pattern_size` is the size of the repeating unit
pub fn create_structured_data(size: usize, pattern_size: usize) -> Vec<u8> {
    assert!(pattern_size > 0, "Pattern size must be > 0");

    let mut rng = StdRng::seed_from_u64(42);

    // Create the base pattern
    let pattern: Vec<u8> = (0..pattern_size).map(|_| rng.r#gen()).collect();

    // Repeat the pattern to fill the size
    let mut data = Vec::with_capacity(size);
    for i in 0..size {
        data.push(pattern[i % pattern_size]);
    }

    data
}

/// Create a temporary directory for tests
pub fn create_temp_dir() -> std::io::Result<TempDir> {
    TempDir::new()
}

/// Create a test file with specified data
pub fn create_test_file_with_data(data: &[u8]) -> std::io::Result<NamedTempFile> {
    let mut file = NamedTempFile::new()?;
    file.write_all(data)?;
    file.flush()?;
    Ok(file)
}

/// Create multiple test files in a directory
pub fn create_test_files(count: usize, size: usize) -> std::io::Result<(TempDir, Vec<PathBuf>)> {
    let dir = TempDir::new()?;
    let mut paths = Vec::new();

    for i in 0..count {
        let path = dir.path().join(format!("test_{i}.dat"));
        let data = create_random_data_with_seed(size, i as u64);
        std::fs::write(&path, &data)?;
        paths.push(path);
    }

    Ok((dir, paths))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_file_zeros() {
        let file = create_test_file(1024, DataPattern::Zeros).unwrap();
        let data = std::fs::read(file.path()).unwrap();
        assert_eq!(data.len(), 1024);
        assert!(data.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_create_random_data() {
        let data1 = create_random_data(1024);
        let data2 = create_random_data(1024);
        assert_eq!(data1.len(), 1024);
        assert_eq!(data2.len(), 1024);
        // Same seed produces same data
        assert_eq!(data1, data2);
    }

    #[test]
    fn test_create_sparse_data() {
        let data = create_sparse_data(10000, 0.9);
        #[allow(clippy::naive_bytecount)]
        let zero_count = data.iter().filter(|&&b| b == 0).count();
        // Should be approximately 90% zeros (within 5% tolerance)
        assert!((zero_count as f64 / 10000.0 - 0.9).abs() < 0.05);
    }

    #[test]
    fn test_create_structured_data() {
        let data = create_structured_data(1000, 10);
        // Verify pattern repeats
        for i in 0..990 {
            assert_eq!(data[i], data[i + 10]);
        }
    }
}
