//! Library crate for the Hexz command-line interface.
//!
//! Re-exports the command handlers and argument definitions for the `hexz`
//! binary. All CLI logic is organized into `cmd` (handlers), `args` (Clap
//! definitions), and `ui` (user interface utilities).

/// Command-line argument definitions (Clap structures).
pub mod args;

/// Command handlers organized by category (data, vm, sys).
///
/// - [`cmd::data`]: Data operations for archives
/// - [`cmd::vm`]: Virtual machine operations
/// - [`cmd::sys`]: System utilities
pub mod cmd;

/// User interface utilities (progress bars, spinners).
///
/// - [`ui::progress`]: Progress bars and spinners for operation feedback
pub mod ui;
