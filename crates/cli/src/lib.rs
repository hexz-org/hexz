//! Library crate for the Strata command-line interface.
//!
//! Re-exports the command handlers and argument definitions for the `strata`
//! binary. All CLI logic is organized into `cmd` (handlers), `args` (Clap
//! definitions), and `ui` (user interface utilities).

/// Command-line argument definitions (Clap structures).
pub mod args;

/// Command handlers organized by category (data, vm, sys).
pub mod cmd;

/// User interface utilities (progress bars, spinners).
pub mod ui;

/// Legacy commands module for backwards compatibility during migration.
#[deprecated(note = "Use cmd module instead")]
pub mod commands;
