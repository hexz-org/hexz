//! Unit tests for strata-core
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

#[path = "unit/dedup/cdc_tests.rs"]
mod cdc_tests;
