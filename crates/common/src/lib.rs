//! Common types, configuration, and constants shared by Hexz crates.
//!
//! This crate centralizes configuration loading, error types, logging setup,
//! and format constants so that the CLI, core, fuse, and server layers
//! share a single source of truth for defaults and wire formats.

/// Configuration loading and runtime parameter management for Hexz tools.
///
/// Exposes the `Config` type used by the CLI and library layers to negotiate
/// defaults (paths, logging, feature flags) in a single place.
pub mod config;

/// Shared constants and format parameters for the Hexz ecosystem.
///
/// Defines magic bytes, format version, header size, block sizes, and
/// codec-related constants used across core, fuse, and CLI crates. Changing
/// these values affects on-disk layout and must be coordinated with format
/// readers and writers.
pub mod constants;

/// Cryptographic utilities shared across crates.
///
/// Houses password-based key-derivation parameters and related helpers that
/// are embedded into snapshot metadata when encryption is enabled.
pub mod crypto;

/// Core error types and result aliases used by Hexz.
///
/// This module defines `Error` and `Result<T>`, which are re-exported at
/// the crate root for ergonomic use in downstream crates.
pub mod error;

/// Logging initialization for the Hexz ecosystem.
///
/// Provides a single entry point for setting up structured logging so that
/// binaries can configure sinks and log levels consistently.
pub mod logging;
#[cfg(feature = "signing")]
pub mod sign;

pub use config::Config;
pub use error::{Error, Result};
