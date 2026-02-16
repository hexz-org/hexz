//! Shared Tokio runtime for async storage backends.
//!
//! Provides a lazily-initialized, process-global Tokio runtime so that
//! `HttpBackend` and `S3Backend` share a single thread pool and reactor
//! instead of each creating their own `Runtime`.

use std::sync::OnceLock;
use tokio::runtime::{Handle, Runtime};

/// Process-global Tokio runtime instance.
static GLOBAL_RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Returns a handle to the shared Tokio runtime.
///
/// The runtime is created lazily on first call with a multi-threaded
/// scheduler. Subsequent calls return a handle to the same runtime.
pub fn global_handle() -> Handle {
    GLOBAL_RUNTIME
        .get_or_init(|| Runtime::new().expect("Failed to create shared Tokio runtime"))
        .handle()
        .clone()
}
