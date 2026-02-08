//! PyO3 binding layer for Strata.
//!
//! This module contains all Python-facing classes and functions.
//! It wraps the pure-Rust `engine` layer with PyO3 annotations.

pub mod async_dataset;
pub mod builder;
pub mod dataset;
pub mod exceptions;
pub mod ops;
pub mod pack;
