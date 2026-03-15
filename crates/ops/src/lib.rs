#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, unused_results))]

//! High-level archive operations for Hexz.

pub mod archive_writer;
pub mod inspect;
pub mod pack;
pub mod parallel_pack;
pub mod parent_index;
/// Pre-pack analysis and savings prediction.
pub mod predict;
pub mod progress;
#[cfg(feature = "signing")]
pub mod sign;
pub mod write;
