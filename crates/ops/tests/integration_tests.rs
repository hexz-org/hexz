//! Integration tests for hexz-ops
//!
//! This module includes all integration tests.

#![allow(dead_code)]

// Common test utilities
#[path = "common/mod.rs"]
mod common;

// Integration tests
#[path = "integration/pack_read_tests.rs"]
mod pack_read_tests;

#[path = "integration/encryption_pack_tests.rs"]
mod encryption_pack_tests;

#[path = "integration/cdc_pack_tests.rs"]
mod cdc_pack_tests;

#[path = "integration/thin_snapshot_tests.rs"]
mod thin_snapshot_tests;

#[path = "integration/file_coverage_tests.rs"]
mod file_coverage_tests;

#[path = "integration/crash_safety_tests.rs"]
mod crash_safety_tests;

#[path = "integration/parallel_pack_tests.rs"]
mod parallel_pack_tests;

#[path = "integration/snapshot_writer_tests.rs"]
mod snapshot_writer_tests;

#[path = "unit/file_tests.rs"]
mod file_tests;
