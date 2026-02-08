//! Zero-copy buffer operations for Python buffer protocol objects.
//!
//! Provides safe wrappers around the CPython buffer protocol C API,
//! enabling zero-copy writes into pre-allocated Python buffers (numpy
//! arrays, bytearrays, etc.) without intermediate allocations.
//!
//! # Safety
//!
//! This module contains `unsafe` code that interfaces directly with
//! CPython's C API. The safety invariants are:
//!
//! 1. **Buffer acquisition**: `PyObject_GetBuffer` is called with
//!    `PyBUF_WRITABLE` to ensure the buffer supports writes. The return
//!    code is checked before assuming the buffer is valid.
//!
//! 2. **Buffer lifetime**: The `BufferInfo` struct owns the `Py_buffer`
//!    and MUST be released via `release_buffer()` before the Python
//!    object it references is dropped. Failure to release causes a
//!    resource leak (but not UB, since CPython handles this gracefully).
//!
//! 3. **Pointer validity**: The `buf` pointer in `Py_buffer` is valid
//!    for `len` bytes as long as the buffer view is held. We never
//!    access beyond `len` bytes.
//!
//! 4. **Thread safety**: Buffer operations must occur while the GIL is
//!    held, which PyO3 guarantees for all `Python<'_>` methods. The
//!    actual data copy in `copy_to_buffer` may occur after GIL release,
//!    but only with data already read from the snapshot (not the buffer
//!    pointer itself — we capture the pointer while GIL is held).

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::os::raw::{c_char, c_int, c_void};

// ---------------------------------------------------------------------------
// CPython buffer protocol FFI definitions
// ---------------------------------------------------------------------------
//
// These are defined manually rather than using PyO3's buffer support because
// PyO3's abi3 feature gates prevent access to `PyBuffer` on stable abi3 builds.
// The struct layout matches CPython's `Py_buffer` exactly.

#[repr(C)]
struct Py_buffer {
    buf: *mut c_void,
    obj: *mut pyo3::ffi::PyObject,
    len: isize,
    itemsize: isize,
    readonly: c_int,
    ndim: c_int,
    format: *mut c_char,
    shape: *mut isize,
    strides: *mut isize,
    suboffsets: *mut isize,
    internal: *mut c_void,
}

unsafe extern "C" {
    fn PyObject_GetBuffer(
        obj: *mut pyo3::ffi::PyObject,
        view: *mut Py_buffer,
        flags: c_int,
    ) -> c_int;
    fn PyBuffer_Release(view: *mut Py_buffer);
}

/// Flag requesting a writable buffer from `PyObject_GetBuffer`.
const PY_BUF_WRITABLE: c_int = 0x0001;

// ---------------------------------------------------------------------------
// Safe wrapper types
// ---------------------------------------------------------------------------

/// Holds a reference to an acquired Python buffer.
///
/// This struct owns the `Py_buffer` and MUST be released via
/// `release_buffer()` when done. It is NOT `Send` or `Sync` because
/// the underlying buffer is tied to a specific Python object and GIL state.
///
/// The `PhantomData<*mut ()>` marker ensures this type is `!Send` and
/// `!Sync` without requiring nightly features.
pub struct BufferInfo {
    view: Py_buffer,
    /// Pointer to the start of the writable buffer region.
    /// Valid for `len` bytes while this `BufferInfo` is alive.
    pub ptr: *mut u8,
    /// Number of writable bytes in the buffer.
    pub len: usize,
    /// Marker to prevent Send/Sync — raw pointers into Python heap are
    /// only valid under the GIL on the thread that acquired them.
    _not_send_sync: std::marker::PhantomData<*mut ()>,
}

/// Acquires a writable buffer view from a Python object.
///
/// This function requests a writable buffer from the Python object using
/// the C buffer protocol. The caller MUST call `release_buffer()` when
/// done, even if subsequent operations fail.
///
/// # Errors
///
/// Returns `PyValueError` if the object does not support the buffer
/// protocol or if the buffer is read-only.
pub fn acquire_writable_buffer(obj: &Bound<'_, PyAny>) -> PyResult<BufferInfo> {
    // SAFETY: We pass a valid PyObject pointer and a pointer to uninitialized
    // memory that PyObject_GetBuffer will fill. We check the return value
    // before using the buffer.
    let mut view = std::mem::MaybeUninit::<Py_buffer>::uninit();

    let res = unsafe { PyObject_GetBuffer(obj.as_ptr(), view.as_mut_ptr(), PY_BUF_WRITABLE) };

    if res != 0 {
        return Err(PyValueError::new_err(
            "buffer is not writable or does not support the buffer protocol",
        ));
    }

    // SAFETY: PyObject_GetBuffer returned 0, so `view` is now initialized.
    let view = unsafe { view.assume_init() };

    Ok(BufferInfo {
        ptr: view.buf as *mut u8,
        len: view.len as usize,
        view,
        _not_send_sync: std::marker::PhantomData,
    })
}

/// Copies data into a previously acquired buffer.
///
/// # Safety
///
/// The caller must ensure:
/// - `buf` was acquired via `acquire_writable_buffer` and has not been released.
/// - `data.len() <= buf.len` (this function clamps to `buf.len` as a defensive measure).
/// - The GIL is held (guaranteed by PyO3 when called from a `#[pymethods]` context).
pub unsafe fn copy_to_buffer(buf: &BufferInfo, data: &[u8]) {
    let copy_len = std::cmp::min(data.len(), buf.len);
    // SAFETY: `buf.ptr` is valid for `buf.len` bytes (guaranteed by successful
    // PyObject_GetBuffer), and `copy_len <= buf.len`. The source slice `data`
    // is a valid Rust reference. The regions do not overlap because `data` is
    // a Rust Vec/slice and `buf.ptr` points into Python's heap.
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr(), buf.ptr, copy_len);
    }
}

/// Releases a previously acquired buffer view.
///
/// This MUST be called for every successful `acquire_writable_buffer`,
/// even if the operation that uses the buffer fails.
pub fn release_buffer(mut buf: BufferInfo) {
    // SAFETY: We are releasing a buffer that was successfully acquired.
    // PyBuffer_Release is idempotent and safe to call once per acquisition.
    unsafe {
        PyBuffer_Release(&mut buf.view);
    }
}
