//! User interface utilities for the Hexz CLI.
//!
//! This module provides consistent, reusable UI components for command-line
//! interactions, including progress bars, spinners, and formatted output.
//!
//! # Design Principles
//!
//! - **Consistency**: All commands use standardized progress indicators
//! - **Accessibility**: Clear visual feedback for long-running operations
//! - **Silent Mode**: UI components respect `--silent` flags
//!
//! # Available Components
//!
//! - [`progress`]: Progress bars and spinners for operation feedback

pub mod help;
pub mod progress;
