// Test helper functions

/// Enhanced byte comparison with better error messages
pub fn assert_bytes_equal(actual: &[u8], expected: &[u8], context: &str) {
    assert!(
        actual.len() == expected.len(),
        "{}: Length mismatch: actual={}, expected={}",
        context,
        actual.len(),
        expected.len()
    );

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

/// Verify data matches expected pattern
pub fn verify_pattern(data: &[u8], pattern: u8) {
    for (i, &byte) in data.iter().enumerate() {
        assert_eq!(
            byte, pattern,
            "Byte mismatch at offset {i}: expected 0x{pattern:02X}, got 0x{byte:02X}"
        );
    }
}

/// Verify sequential pattern (0, 1, 2, ..., 255, 0, 1, ...)
pub fn verify_sequential(data: &[u8]) {
    for (i, &byte) in data.iter().enumerate() {
        let expected = (i % 256) as u8;
        assert_eq!(
            byte, expected,
            "Byte mismatch at offset {i}: expected 0x{expected:02X}, got 0x{byte:02X}"
        );
    }
}

/// Check if data is all zeros
pub fn is_all_zeros(data: &[u8]) -> bool {
    data.iter().all(|&b| b == 0)
}

/// Check if data is all ones (0xFF)
pub fn is_all_ones(data: &[u8]) -> bool {
    data.iter().all(|&b| b == 0xFF)
}
