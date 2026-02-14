//! FFI bindings to the SpiceDB C shared library (built from Go or downloaded per target).
//!
//! When the server is started with `grpc_transport: "memory"`, use [`memory`] for safe, typed RPCs
//! or the raw C functions below for marshalled protobuf bytes.

mod instance;
mod memory;

pub use instance::{dispose, start};
pub use memory::{RpcError, RpcResult};

pub mod memory_transport {
    //! Safe, typed wrappers for memory-transport RPCs (marshall/unmarshall in this crate).
    pub use crate::memory::{
        RpcError, RpcResult, check_bulk_permissions, check_permission, delete_relationships,
        expand_permission_tree, read_schema, write_relationships, write_schema,
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

    /// Invokes PermissionsService.CheckPermission. Request/response are marshalled protobuf (authzed.api.v1).
    /// On success: `*out_response_bytes` and `*out_response_len` are set; caller frees `*out_response_bytes` with `spicedb_free_bytes`.
    /// On error: `*out_response_bytes` is NULL, `*out_error` is set; caller frees `*out_error` with `spicedb_free`.
    pub(crate) fn spicedb_permissions_check_permission(
        handle: c_ulonglong,
        request_bytes: *const c_uchar,
        request_len: c_int,
        out_response_bytes: *mut *mut c_uchar,
        out_response_len: *mut c_int,
        out_error: *mut *mut c_char,
    );

    /// Invokes PermissionsService.WriteRelationships. Same ABI as other RPC FFI functions.
    pub(crate) fn spicedb_permissions_write_relationships(
        handle: c_ulonglong,
        request_bytes: *const c_uchar,
        request_len: c_int,
        out_response_bytes: *mut *mut c_uchar,
        out_response_len: *mut c_int,
        out_error: *mut *mut c_char,
    );

    /// Invokes SchemaService.WriteSchema. Same ABI as other RPC FFI functions.
    pub(crate) fn spicedb_schema_write_schema(
        handle: c_ulonglong,
        request_bytes: *const c_uchar,
        request_len: c_int,
        out_response_bytes: *mut *mut c_uchar,
        out_response_len: *mut c_int,
        out_error: *mut *mut c_char,
    );

    pub(crate) fn spicedb_permissions_delete_relationships(
        handle: c_ulonglong,
        request_bytes: *const c_uchar,
        request_len: c_int,
        out_response_bytes: *mut *mut c_uchar,
        out_response_len: *mut c_int,
        out_error: *mut *mut c_char,
    );

    pub(crate) fn spicedb_permissions_check_bulk_permissions(
        handle: c_ulonglong,
        request_bytes: *const c_uchar,
        request_len: c_int,
        out_response_bytes: *mut *mut c_uchar,
        out_response_len: *mut c_int,
        out_error: *mut *mut c_char,
    );

    pub(crate) fn spicedb_permissions_expand_permission_tree(
        handle: c_ulonglong,
        request_bytes: *const c_uchar,
        request_len: c_int,
        out_response_bytes: *mut *mut c_uchar,
        out_response_len: *mut c_int,
        out_error: *mut *mut c_char,
    );

    pub(crate) fn spicedb_schema_read_schema(
        handle: c_ulonglong,
        request_bytes: *const c_uchar,
        request_len: c_int,
        out_response_bytes: *mut *mut c_uchar,
        out_response_len: *mut c_int,
        out_error: *mut *mut c_char,
    );
}
