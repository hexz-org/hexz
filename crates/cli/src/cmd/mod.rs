//! Command handlers for the Strata CLI.
//!
//! This module organizes subcommands into logical groups:
//! - `data`: Data operations (pack, inspect, diff, analyze)
//! - `vm`: Virtual machine operations (boot, install, snap, commit, mount)
//! - `sys`: System utilities (doctor, bench, serve, keygen, sign, verify)

pub mod data;
pub mod sys;
pub mod vm;
