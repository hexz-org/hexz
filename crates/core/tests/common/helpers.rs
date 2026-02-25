// Test helper functions (no external crate dependencies)
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
