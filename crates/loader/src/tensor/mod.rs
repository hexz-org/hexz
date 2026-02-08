//! Tensor and buffer utilities for zero-copy data transfer.
//!
//! This module provides efficient mechanisms for transferring snapshot
//! data into Python buffer protocol objects (numpy arrays, bytearrays)
//! without unnecessary copies.

pub mod numpy;
