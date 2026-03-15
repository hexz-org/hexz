//! ANSI color palette for CLI output.
//!
//! Returns populated escape sequences when stdout is an interactive terminal
//! and `NO_COLOR` is unset; otherwise returns empty strings so output remains
//! plain for pipes, redirects, and CI environments.

use std::io::IsTerminal;

/// A set of ANSI escape sequences (or empty strings when color is disabled).
#[derive(Debug)]
pub struct Palette {
    /// Bold text.
    pub bold: &'static str,
    /// Dim text.
    pub dim: &'static str,
    /// Reset all attributes.
    pub reset: &'static str,
    /// Bright cyan — used for label keys.
    pub cyan: &'static str,
    /// Bright green — used for file sizes and success status.
    pub green: &'static str,
    /// Bright yellow — used for annotations and scalar values.
    pub yellow: &'static str,
    /// Dark gray — used for tree chrome and auxiliary info.
    pub gray: &'static str,
    /// Bright red — used for error/failure status.
    pub red: &'static str,
}

static COLORS: Palette = Palette {
    bold: "\x1b[1m",
    dim: "\x1b[2m",
    reset: "\x1b[0m",
    cyan: "\x1b[96m",
    green: "\x1b[92m",
    yellow: "\x1b[93m",
    gray: "\x1b[90m",
    red: "\x1b[91m",
};

static PLAIN: Palette = Palette {
    bold: "",
    dim: "",
    reset: "",
    cyan: "",
    green: "",
    yellow: "",
    gray: "",
    red: "",
};

/// Returns the color palette appropriate for stdout.
///
/// Emits ANSI codes if stdout is a terminal and `NO_COLOR` is not set;
/// otherwise every field is an empty string.
pub fn palette() -> &'static Palette {
    if std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none() {
        &COLORS
    } else {
        &PLAIN
    }
}
