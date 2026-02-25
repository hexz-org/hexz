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

#[path = "unit/storage/file_backend_tests.rs"]
mod file_backend_tests;

#[path = "unit/storage/mmap_backend_tests.rs"]
mod mmap_backend_tests;

#[path = "unit/storage/http_backend_simple_tests.rs"]
mod http_backend_simple_tests;

#[path = "unit/dedup/cdc_tests.rs"]
mod cdc_tests;

#[path = "unit/file_tests.rs"]
mod file_tests;

#[path = "unit/http_mock_tests.rs"]
mod http_mock_tests;

#[path = "unit/storage/s3_backend_tests.rs"]
mod s3_backend_tests;

// TODO: Fix API imports
// #[path = "unit/dedup/dcam_tests.rs"]
// mod dcam_tests;

// TODO: Fix API imports
// #[path = "unit/writer_tests.rs"]
// mod writer_tests;

// TODO: Fix API imports
// #[path = "unit/header_tests.rs"]
// mod header_tests;
