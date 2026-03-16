#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_results,
    clippy::unreadable_literal,
    clippy::significant_drop_tightening,
    clippy::needless_pass_by_value,
    clippy::float_cmp
)]
//! Tests for hexz-store storage backends

#![allow(dead_code)]

// Common test utilities
#[path = "common/mod.rs"]
mod common;

#[path = "unit/storage/file_backend_tests.rs"]
mod file_backend_tests;

#[path = "unit/storage/http_backend_simple_tests.rs"]
mod http_backend_simple_tests;

#[path = "unit/storage/http_backend_tests.rs"]
mod http_backend_tests;

#[path = "unit/storage/s3_backend_tests.rs"]
mod s3_backend_tests;

#[path = "unit/http_mock_tests.rs"]
mod http_mock_tests;
