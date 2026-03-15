#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, unused_results))]

//! Library crate for the Hexz command-line interface.
//!
//! Re-exports the command handlers and argument definitions for the `hexz`
//! binary. All CLI logic is organized into `cmd` (handlers), `args` (Clap
//! definitions), and `ui` (user interface utilities).

/// Command-line argument definitions (Clap structures).
pub mod args;

/// Command handlers organized by category (data, vm, sys).
///
/// This module contains all the CLI command implementations:
///
/// - `data`: Data operations (pack, info, diff, analyze)
/// - `vm`: Virtual machine operations (boot, commit, archive)
/// - `sys`: System utilities (mount, unmount)
pub mod cmd;

/// User interface utilities (progress bars, spinners).
///
/// Provides user-facing progress indicators for long-running operations:
///
/// - `progress`: Progress bars and spinners
/// - Consistent styling across all commands
pub mod ui;
