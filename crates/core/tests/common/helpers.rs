// Test helper functions
use std::sync::Arc;

/// Enhanced byte comparison with better error messages
pub fn assert_bytes_equal(actual: &[u8], expected: &[u8], context: &str) {
    if actual.len() != expected.len() {
        panic!(
            "{}: Length mismatch: actual={}, expected={}",
            context,
            actual.len(),
            expected.len()
        );
    }

    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        if a != e {
            let start = i.saturating_sub(8);
            let end = (i + 8).min(actual.len());
            panic!(
                "{}: Byte mismatch at offset {}\n\
                 Expected: 0x{:02X}\n\
                 Actual:   0x{:02X}\n\
                 Context (offset {}-{}):\n\
                 Expected: {:02X?}\n\
                 Actual:   {:02X?}",
                context,
                i,
                e,
                a,
                start,
                end,
                &expected[start..end],
                &actual[start..end]
            );
        }
    }
}

/// Measure compression ratio
pub fn measure_compression_ratio(original_size: usize, compressed_size: usize) -> f64 {
    if original_size == 0 {
        return 0.0;
    }
    compressed_size as f64 / original_size as f64
}

/// Calculate entropy of data (in bits per byte)
pub fn calculate_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    let mut counts = [0u64; 256];
    for &byte in data {
        counts[byte as usize] += 1;
    }

    let len = data.len() as f64;
    let mut entropy = 0.0;

    for &count in &counts {
        if count > 0 {
            let p = count as f64 / len;
            entropy -= p * p.log2();
        }
    }

    entropy
}

/// Create a mock in-memory backend for testing
pub struct MockBackend {
    data: Arc<Vec<u8>>,
}

impl MockBackend {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data: Arc::new(data),
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn read(&self, offset: usize, buf: &mut [u8]) -> std::io::Result<usize> {
        if offset >= self.data.len() {
            return Ok(0);
        }

        let available = self.data.len() - offset;
        let to_read = available.min(buf.len());
        buf[..to_read].copy_from_slice(&self.data[offset..offset + to_read]);
        Ok(to_read)
    }
}

/// Verify data matches expected pattern
pub fn verify_pattern(data: &[u8], pattern: u8) {
    for (i, &byte) in data.iter().enumerate() {
        assert_eq!(
            byte, pattern,
            "Byte mismatch at offset {}: expected 0x{:02X}, got 0x{:02X}",
            i, pattern, byte
        );
    }
}

/// Verify sequential pattern (0, 1, 2, ..., 255, 0, 1, ...)
pub fn verify_sequential(data: &[u8]) {
    for (i, &byte) in data.iter().enumerate() {
        let expected = (i % 256) as u8;
        assert_eq!(
            byte, expected,
            "Byte mismatch at offset {}: expected 0x{:02X}, got 0x{:02X}",
            i, expected, byte
        );
    }
}

/// Compare two byte slices and return first differing offset
pub fn find_first_difference(a: &[u8], b: &[u8]) -> Option<usize> {
    a.iter().zip(b.iter()).position(|(x, y)| x != y)
}

/// Check if data is all zeros
pub fn is_all_zeros(data: &[u8]) -> bool {
    data.iter().all(|&b| b == 0)
}

/// Check if data is all ones (0xFF)
pub fn is_all_ones(data: &[u8]) -> bool {
    data.iter().all(|&b| b == 0xFF)
}

/// Count occurrences of a specific byte
pub fn count_byte(data: &[u8], byte: u8) -> usize {
    data.iter().filter(|&&b| b == byte).count()
}

/// Calculate percentage of zeros in data
pub fn zero_percentage(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let zero_count = count_byte(data, 0);
    (zero_count as f64 / data.len() as f64) * 100.0
}

/// Generate a random password for testing
pub fn random_password() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..16)
        .map(|_| {
            let c = rng.gen_range(b'a'..=b'z');
            c as char
        })
        .collect()
}

/// Create a simple snapshot for testing using PackConfig
pub fn create_simple_snapshot() -> Result<(std::path::PathBuf, Vec<u8>), Box<dyn std::error::Error>>
{
    use std::fs;
    use strata_core::ops::pack::{PackConfig, pack_snapshot};
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let data = (0..1024 * 1024)
        .map(|i| (i % 256) as u8)
        .collect::<Vec<u8>>();
    let disk_path = temp_dir.path().join("disk.img");
    fs::write(&disk_path, &data)?;

    let snap_path = temp_dir.path().join("snapshot.st");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: snap_path.clone(),
        compression: "lz4".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        min_chunk: 16384,
        avg_chunk: 65536,
        max_chunk: 131072,
    };

    pack_snapshot(config, None::<fn(u64, u64)>)?;

    // Persist the temp dir by leaking it (files needed for test lifetime)
    let persisted = temp_dir.keep();
    let final_snap = persisted.join("snapshot.st");

    Ok((final_snap, data))
}

/// Create a snapshot with memory data using PackConfig
pub fn create_snapshot_with_memory()
-> Result<(std::path::PathBuf, Vec<u8>), Box<dyn std::error::Error>> {
    use std::fs;
    use strata_core::ops::pack::{PackConfig, pack_snapshot};
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let disk_data = (0..512 * 1024)
        .map(|i| (i % 256) as u8)
        .collect::<Vec<u8>>();
    let mem_data = vec![0xAA; 256 * 1024];

    let disk_path = temp_dir.path().join("disk.img");
    let mem_path = temp_dir.path().join("mem.img");
    fs::write(&disk_path, &disk_data)?;
    fs::write(&mem_path, &mem_data)?;

    let snap_path = temp_dir.path().join("snapshot.st");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: Some(mem_path),
        output: snap_path.clone(),
        compression: "lz4".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        min_chunk: 16384,
        avg_chunk: 65536,
        max_chunk: 131072,
    };

    pack_snapshot(config, None::<fn(u64, u64)>)?;

    let persisted = temp_dir.keep();
    let final_snap = persisted.join("snapshot.st");

    Ok((final_snap, disk_data))
}

/// Create a multi-block snapshot for testing parallel decompression
pub fn create_multi_block_snapshot()
-> Result<(std::path::PathBuf, Vec<u8>), Box<dyn std::error::Error>> {
    use std::fs;
    use strata_core::ops::pack::{PackConfig, pack_snapshot};
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let data = (0..512 * 1024)
        .map(|i| (i % 256) as u8)
        .collect::<Vec<u8>>();
    let disk_path = temp_dir.path().join("disk.img");
    fs::write(&disk_path, &data)?;

    let snap_path = temp_dir.path().join("snapshot.st");

    let config = PackConfig {
        disk: Some(disk_path),
        memory: None,
        output: snap_path.clone(),
        compression: "lz4".to_string(),
        encrypt: false,
        password: None,
        train_dict: false,
        block_size: 65536,
        cdc_enabled: false,
        min_chunk: 16384,
        avg_chunk: 65536,
        max_chunk: 131072,
    };

    pack_snapshot(config, None::<fn(u64, u64)>)?;

    let persisted = temp_dir.keep();
    let final_snap = persisted.join("snapshot.st");

    Ok((final_snap, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assert_bytes_equal_success() {
        let a = vec![1, 2, 3, 4, 5];
        let b = vec![1, 2, 3, 4, 5];
        assert_bytes_equal(&a, &b, "test");
    }

    #[test]
    #[should_panic(expected = "Length mismatch")]
    fn test_assert_bytes_equal_length_mismatch() {
        let a = vec![1, 2, 3];
        let b = vec![1, 2, 3, 4];
        assert_bytes_equal(&a, &b, "test");
    }

    #[test]
    fn test_measure_compression_ratio() {
        assert_eq!(measure_compression_ratio(1000, 500), 0.5);
        assert_eq!(measure_compression_ratio(1000, 1000), 1.0);
        assert_eq!(measure_compression_ratio(0, 0), 0.0);
    }

    #[test]
    fn test_calculate_entropy() {
        // All zeros should have 0 entropy
        let zeros = vec![0u8; 1000];
        assert!(calculate_entropy(&zeros) < 0.1);

        // Uniform distribution should have ~8.0 entropy
        let uniform: Vec<u8> = (0..256).cycle().take(10000).map(|x| x as u8).collect();
        let entropy = calculate_entropy(&uniform);
        assert!(entropy > 7.5 && entropy <= 8.0);
    }

    #[test]
    fn test_mock_backend() {
        let data = vec![1, 2, 3, 4, 5];
        let backend = MockBackend::new(data);
        assert_eq!(backend.len(), 5);
        assert!(!backend.is_empty());

        let mut buf = vec![0u8; 3];
        let n = backend.read(1, &mut buf).unwrap();
        assert_eq!(n, 3);
        assert_eq!(buf, vec![2, 3, 4]);
    }

    #[test]
    fn test_verify_pattern() {
        let data = vec![0x42; 100];
        verify_pattern(&data, 0x42);
    }

    #[test]
    fn test_verify_sequential() {
        let data: Vec<u8> = (0..512).map(|i| (i % 256) as u8).collect();
        verify_sequential(&data);
    }

    #[test]
    fn test_is_all_zeros() {
        assert!(is_all_zeros(&[0, 0, 0]));
        assert!(!is_all_zeros(&[0, 1, 0]));
    }

    #[test]
    fn test_zero_percentage() {
        let data = vec![0, 0, 0, 1, 1];
        assert_eq!(zero_percentage(&data), 60.0);
    }
}
