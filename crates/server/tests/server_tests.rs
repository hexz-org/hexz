#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_results,
    clippy::unreadable_literal,
    clippy::significant_drop_tightening,
    clippy::needless_pass_by_value,
    clippy::float_cmp
)]
//! Unit tests for server utilities (`parse_range`, constants).

use hexz_server::parse_range;

#[test]
fn test_parse_range_valid_bounded() {
    assert_eq!(parse_range("bytes=0-1023", 10000), Some((0, 1023)));
}

#[test]
fn test_parse_range_valid_unbounded() {
    assert_eq!(parse_range("bytes=1024-", 10000), Some((1024, 9999)));
}

#[test]
fn test_parse_range_single_byte() {
    assert_eq!(parse_range("bytes=0-0", 1000), Some((0, 0)));
}

#[test]
fn test_parse_range_at_eof() {
    // bytes=0-999 with size=1000 means last valid byte is 999
    assert_eq!(parse_range("bytes=0-999", 1000), Some((0, 999)));
}

#[test]
fn test_parse_range_missing_prefix() {
    assert_eq!(parse_range("0-1023", 10000), None);
}

#[test]
fn test_parse_range_invalid_integers() {
    assert_eq!(parse_range("bytes=abc-def", 10000), None);
}

#[test]
fn test_parse_range_inverted_range() {
    assert_eq!(parse_range("bytes=1000-500", 10000), None);
}

#[test]
fn test_parse_range_out_of_bounds() {
    // size=1000, valid range is 0-999; requesting 0-1000 is out of bounds
    assert_eq!(parse_range("bytes=0-1000", 1000), None);
}

#[test]
fn test_parse_range_suffix_unsupported() {
    assert_eq!(parse_range("bytes=-500", 10000), None);
}

#[test]
fn test_parse_range_empty_after_prefix() {
    assert_eq!(parse_range("bytes=", 10000), None);
}

#[test]
fn test_parse_range_constants() {
    // MAX_CHUNK_SIZE = 32 * 1024 * 1024 = 33554432
    // RANGE_PREFIX_LEN = 6
    // These are private constants, but we test them indirectly:
    // "bytes=" is 6 characters, verified by all prefix-dependent tests above.

    // Verify the parse logic handles large sizes correctly
    let large_size = 10 * 1024 * 1024 * 1024u64; // 10 GiB
    assert_eq!(parse_range("bytes=0-1023", large_size), Some((0, 1023)));
}

#[test]
fn test_parse_range_mid_range() {
    assert_eq!(parse_range("bytes=500-1499", 5000), Some((500, 1499)));
}

#[test]
fn test_parse_range_unbounded_at_zero() {
    assert_eq!(parse_range("bytes=0-", 1000), Some((0, 999)));
}
