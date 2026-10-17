//! FFI bindings to the SpiceDB C shared library (built from Go or downloaded per target).
//!
//! Provides an in-memory server and raw C functions that take in marshalled gRPC requests and
//! return marshalled responses or error strings.

mod instance;
mod memory;

pub use instance::{dispose, start};
pub use memory::{RpcError, RpcResult};

pub mod memory_transport {
    //! Safe, typed wrappers for memory-transport RPCs (marshall/unmarshall in this crate).
    pub use crate::memory::{
        RpcError, RpcResult, TraceContext, call, check_bulk_permissions, check_permission,
        delete_relationships, expand_permission_tree, methods, read_schema, write_relationships,
        write_schema,
    };
}

use std::os::raw::{c_char, c_int, c_uchar, c_ulonglong};

#[link(name = "spicedb")]
unsafe extern "C" {
    pub(crate) fn spicedb_start(options_json: *const c_char) -> *mut c_char;
    pub(crate) fn spicedb_dispose(handle: c_ulonglong) -> *mut c_char;
    pub(crate) fn spicedb_free(ptr: *mut c_char);

    /// Frees a byte buffer returned by RPC FFI functions.
    pub(crate) fn spicedb_free_bytes(ptr: *mut std::ffi::c_void);

    /// Invokes a unary method by its full gRPC name, e.g. `/authzed.api.v1.PermissionsService/CheckPermission`.
    /// Request/response are marshalled protobuf (authzed.api.v1). `traceparent` and `tracestate` are
    /// W3C trace context headers, or NULL; SpiceDB's server span joins that trace.
    /// On success: `*out_response_bytes` and `*out_response_len` are set; caller frees `*out_response_bytes` with `spicedb_free_bytes`.
    /// On error: `*out_response_bytes` is NULL, `*out_error` is set; caller frees `*out_error` with `spicedb_free`.
    pub(crate) fn spicedb_rpc(
        handle: c_ulonglong,
        method: *const c_char,
        traceparent: *const c_char,
        tracestate: *const c_char,
        request_bytes: *const c_uchar,
        request_len: c_int,
        out_response_bytes: *mut *mut c_uchar,
        out_response_len: *mut c_int,
        out_error: *mut *mut c_char,
    );
}
