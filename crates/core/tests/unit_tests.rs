#![allow(clippy::unwrap_used, clippy::expect_used, unused_results, clippy::unreadable_literal, clippy::significant_drop_tightening, clippy::needless_pass_by_value, clippy::float_cmp)]
//! Unit tests for hexz-core
//!
//! This module includes all unit tests organized by functionality.

#![allow(dead_code)]

// Common test utilities
#[path = "common/mod.rs"]
mod common;

// Unit tests
#[path = "unit/cache_tests.rs"]
mod cache_tests;

#[path = "unit/compression_tests.rs"]
mod compression_tests;

#[path = "unit/encryption_tests.rs"]
mod encryption_tests;

#[path = "unit/fixtures_tests.rs"]
mod fixtures_tests;

#[path = "unit/format_tests.rs"]
mod format_tests;

#[path = "unit/dedup/cdc_tests.rs"]
mod cdc_tests;

// DCAM tests live in integration tests (require a full file for analysis).

// Writer tests live in hexz-ops (ArchiveWriter is defined there).

#[path = "unit/header_tests.rs"]
mod header_tests;
