//! FFI bindings to the prebuilt SpiceDB C shared library for macOS aarch64.

use std::os::raw::{c_char, c_ulonglong};

#[link(name = "spicedb")]
unsafe extern "C" {
    pub fn spicedb_start(options_json: *const c_char) -> *mut c_char;
    pub fn spicedb_dispose(handle: c_ulonglong) -> *mut c_char;
    pub fn spicedb_free(ptr: *mut c_char);
}
