#![allow(clippy::unwrap_used, clippy::expect_used, unused_results, clippy::unreadable_literal, clippy::significant_drop_tightening, clippy::needless_pass_by_value, clippy::float_cmp)]
//! Property-based tests for hexz-core
//!
//! Uses proptest to verify invariants across random inputs.

#![allow(dead_code)]

// Common test utilities
#[path = "common/mod.rs"]
mod common;

// Property-based tests
#[path = "property/integrity_tests.rs"]
mod integrity_tests;
