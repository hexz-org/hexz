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
