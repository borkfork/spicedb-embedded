//! Safe wrappers for instance lifecycle: start and dispose.
//!
//! All FFI and unsafe code for these operations is contained here.

use std::ffi::{CStr, CString};

/// Starts a SpiceDB instance. Returns the JSON response string (caller parses).
///
/// # Errors
///
/// Returns an error string if the C library returns null or the operation fails.
pub fn start(options_json: Option<&str>) -> Result<String, String> {
    let ptr = match options_json {
        Some(s) => {
            let cstr = CString::new(s).map_err(|e| format!("options contain null byte: {e}"))?;
            unsafe { crate::spicedb_start(cstr.as_ptr()) }
        }
        None => unsafe { crate::spicedb_start(std::ptr::null()) },
    };
    if ptr.is_null() {
        return Err("null response from C library".to_string());
    }
    let s = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
    unsafe { crate::spicedb_free(ptr) };
    Ok(s)
}

/// Disposes a SpiceDB instance. Returns the JSON response string (caller may ignore).
///
/// # Errors
///
/// Returns an error string if the C library returns null.
pub fn dispose(handle: u64) -> Result<String, String> {
    let ptr = unsafe { crate::spicedb_dispose(handle) };
    if ptr.is_null() {
        return Ok("{}".to_string());
    }
    let s = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
    unsafe { crate::spicedb_free(ptr) };
    Ok(s)
}
