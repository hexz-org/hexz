//! Public API for snapshot file access.
//!
//! Re-exports the `stratafile` module, which defines `StrataFile` and
//! `SnapshotStream` for reading logical disk and memory streams from
//! a `.st` file.

/// High-level snapshot file API.
///
/// Exposes `StrataFile` and related types that present logical disk and memory
/// streams backed by the on-disk snapshot format.
pub mod stratafile;
