//! Integration tests for strata-core
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

#[path = "integration/stratafile_coverage_tests.rs"]
mod stratafile_coverage_tests;
