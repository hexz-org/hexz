//! Deduplication algorithms.
//!
//! Content-defined chunking (CDC) and the Deduplication Change-Estimation
//! Analytical Model (DCAM) for optimizing dedup parameters.

pub mod cdc;
pub mod dcam;

pub use cdc::{CdcStats, StreamChunker};
pub use dcam::DedupeParams;
