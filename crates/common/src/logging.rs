//! Logging Infrastructure Initialization.
//!
//! This module handles the setup of the application's logging infrastructure.
//! It configures the tracing subscriber to output structured logs to the console,
//! enabling observability for debugging and operational monitoring.

/// Initializes the global tracing subscriber.
///
/// This function sets up the default logging output format and level filter.
/// It must be called once at the start of the application (e.g., in `main`)
/// to ensure that log events from libraries and the application itself are
/// captured and displayed. The subscriber uses the default format which
/// includes timestamps, log levels, and message content.
///
/// # Panics
///
/// This function may panic if the tracing subscriber has already been initialized,
/// as Rust's tracing system only allows a single global subscriber.
pub fn init() {
    tracing_subscriber::fmt::init();
}
