//! Shared Tokio runtime for async storage backends.
//!
//! Lazily initialises a process-global multi-threaded runtime so that
//! `HttpBackend` and `S3Backend` share one thread pool rather than each
//! creating their own.

use std::io;
use std::sync::OnceLock;
use tokio::runtime::{Handle, Runtime};

static GLOBAL_RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Returns a handle to the shared Tokio runtime, creating it on first call.
///
/// # Errors
///
/// Returns an I/O error if the Tokio runtime cannot be created (e.g. the OS
/// cannot spawn worker threads).
pub fn global_handle() -> io::Result<Handle> {
    // OnceLock::get_or_init is infallible and doesn't propagate errors from
    // the init closure, so we try to create the runtime and store it only on
    // success.  On repeated calls we just hand back the existing handle.
    if let Some(rt) = GLOBAL_RUNTIME.get() {
        return Ok(rt.handle().clone());
    }
    let rt = Runtime::new()?;
    // Another thread may have raced us; that's fine — get_or_init returns
    // whichever value was stored first.
    Ok(GLOBAL_RUNTIME.get_or_init(|| rt).handle().clone())
}
