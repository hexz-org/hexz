use libc::{c_char, c_void, size_t};
use std::ffi::CStr;
use std::ptr;

/// Opaque handle to a Strata writer.
pub struct StrataWriter {
    // Placeholder for writer state
    // inner: Box<dyn Writer>,
}

/// Creates a new Strata writer.
///
/// # Safety
/// `path` must be a null-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strata_writer_new(path: *const c_char) -> *mut StrataWriter {
    if path.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: Caller must ensure path is a valid null-terminated C string
    let _path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    // TODO: Initialize the actual Rust `StrataWriter` (from strata-core).
    // let writer = StrataWriter::new(path_str).expect("Failed to create writer");
    // Box::into_raw(Box::new(writer))

    // Placeholder implementation
    Box::into_raw(Box::new(StrataWriter {}))
}

/// Appends data to the writer.
///
/// # Safety
/// `writer` must be a valid pointer from `strata_writer_new`.
/// `buf` must be valid for `len` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn strata_writer_append(
    writer: *mut StrataWriter,
    buf: *const c_void,
    _len: size_t,
) -> i32 {
    if writer.is_null() || buf.is_null() {
        return -1;
    }
    // TODO: Convert C buffer to Rust slice and write.
    // let slice = unsafe { std::slice::from_raw_parts(buf as *const u8, len) };
    // unsafe { (*writer).write(slice).map(|_| 0).unwrap_or(-1) }

    // Placeholder
    0
}

/// Closes and frees the writer.
///
/// # Safety
/// `writer` must be a valid pointer. It is invalidated after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strata_writer_finish(writer: *mut StrataWriter) -> i32 {
    if writer.is_null() {
        return -1;
    }
    // SAFETY: Caller must ensure writer is a valid pointer from strata_writer_new
    let _ = unsafe { Box::from_raw(writer) };
    0
}
