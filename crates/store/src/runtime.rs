//! Shared Tokio runtime for async storage backends.
//!
//! Lazily initialises a process-global multi-threaded runtime so that
//! `HttpBackend` and `S3Backend` share one thread pool rather than each
//! creating their own.

use std::sync::OnceLock;
use tokio::runtime::{Handle, Runtime};

static GLOBAL_RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Returns a handle to the shared Tokio runtime, creating it on first call.
pub fn global_handle() -> Handle {
    GLOBAL_RUNTIME
        .get_or_init(|| Runtime::new().expect("Failed to create shared Tokio runtime"))
        .handle()
        .clone()
}
