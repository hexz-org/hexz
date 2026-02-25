// Test helper functions

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
