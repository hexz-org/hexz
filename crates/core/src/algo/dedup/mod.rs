//! Deduplication algorithms and parameter optimization.
//!
//! This module provides content-defined chunking (CDC) for variable-sized blocks
//! and the Deduplication Change-Estimation Analytical Model (DCAM) for scientifically
//! optimizing CDC parameters.
//!
//! # Content-Defined Chunking (CDC)
//!
//! CDC uses a rolling hash to create variable-sized blocks that align with content
//! boundaries rather than fixed offsets. This dramatically improves deduplication
//! when data is inserted or deleted (the "boundary-shift problem").
//!
//! **Key Properties:**
//! - Block boundaries depend only on content, not position
//! - Inserting/deleting bytes only affects nearby blocks
//! - Average block size controlled by fingerprint parameter
//!
//! ## FastCDC Algorithm
//!
//! Strata uses **FastCDC**, which combines:
//! - **Gear hash**: Fast rolling hash (no modulo, just bit masks)
//! - **Normalized chunking**: Maintains target size better than basic CDC
//! - **Min/max enforcement**: Prevents pathologically small/large chunks
//!
//! # DCAM (Parameter Optimization)
//!
//! DCAM is an analytical model that predicts deduplication effectiveness without
//! performing full chunking. It enables fast parameter optimization by:
//!
//! 1. Sampling data with baseline parameters
//! 2. Calculating change probability (`c`)
//! 3. Testing parameter combinations analytically
//! 4. Recommending optimal min/avg/max chunk sizes
//!
//! **Usage:** Run `strata data analyze` before creating archives to find optimal
//! parameters for your dataset.
//!
//! # Trade-offs
//!
//! | Aspect | Fixed-Size Blocks | CDC (FastCDC) |
//! |--------|-------------------|---------------|
//! | Speed | ✓ Faster (no rolling hash) | ✗ Slower (~10-20% overhead) |
//! | Dedup (static data) | ✓ Same | ✓ Same |
//! | Dedup (inserted data) | ✗ Poor (boundaries shift) | ✓ Excellent |
//! | Complexity | ✓ Simple | ✗ More complex |
//!
//! **Recommendation:** Use CDC for versioned data (VMs, containers), fixed-size
//! for write-once archives (backups, media files).

pub mod cdc;
pub mod dcam;

pub use cdc::{CdcStats, StreamChunker};
pub use dcam::DedupeParams;
