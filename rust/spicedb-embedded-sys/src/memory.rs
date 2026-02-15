//! Safe, typed wrappers for the memory-transport FFI.
//!
//! All marshalling/unmarshalling and unsafe FFI calls are contained here.
//! The main crate uses these functions for idiomatic, safe APIs.

use std::os::raw::{c_char, c_int, c_uchar, c_ulonglong};

use prost::Message;
use spicedb_grpc_tonic::v1::{
    CheckBulkPermissionsRequest, CheckBulkPermissionsResponse, CheckPermissionRequest,
    CheckPermissionResponse, DeleteRelationshipsRequest, DeleteRelationshipsResponse,
    ExpandPermissionTreeRequest, ExpandPermissionTreeResponse, ReadSchemaRequest,
    ReadSchemaResponse, WriteRelationshipsRequest, WriteRelationshipsResponse, WriteSchemaRequest,
    WriteSchemaResponse,
};

/// Error from a memory-transport RPC (FFI or decode).
#[derive(Debug, Clone)]
pub struct RpcError(pub String);

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for RpcError {}

/// Result type for memory-transport RPCs.
pub type RpcResult<T> = Result<T, RpcError>;

/// Raw call: request bytes in, response bytes or error string. Handles all unsafe FFI and buffer freeing.
fn raw_call(
    handle: u64,
    request_bytes: &[u8],
    f: unsafe extern "C" fn(
        c_ulonglong,
        *const c_uchar,
        c_int,
        *mut *mut c_uchar,
        *mut c_int,
        *mut *mut c_char,
    ),
) -> RpcResult<Vec<u8>> {
    let mut out_response_bytes: *mut c_uchar = std::ptr::null_mut();
    let mut out_response_len: c_int = 0;
    let mut out_error: *mut c_char = std::ptr::null_mut();

    unsafe {
        f(
            handle as c_ulonglong,
            request_bytes.as_ptr(),
            request_bytes.len() as c_int,
            &mut out_response_bytes,
            &mut out_response_len,
            &mut out_error,
        );
    }

    if !out_error.is_null() {
        let s = unsafe { std::ffi::CStr::from_ptr(out_error) }
            .to_string_lossy()
            .into_owned();
        unsafe { crate::spicedb_free(out_error) };
        return Err(RpcError(s));
    }

    let len = out_response_len as usize;
    let bytes = if len == 0 {
        vec![]
    } else {
        unsafe { std::slice::from_raw_parts(out_response_bytes, len).to_vec() }
    };
    if !out_response_bytes.is_null() {
        unsafe { crate::spicedb_free_bytes(out_response_bytes as *mut std::ffi::c_void) };
    }
    Ok(bytes)
}

// --- PermissionsService ---

/// PermissionsService.CheckPermission.
pub fn check_permission(
    handle: u64,
    request: &CheckPermissionRequest,
) -> RpcResult<CheckPermissionResponse> {
    let mut buf = Vec::new();
    request
        .encode(&mut buf)
        .map_err(|e| RpcError(e.to_string()))?;
    let bytes = raw_call(handle, &buf, crate::spicedb_permissions_check_permission)?;
    CheckPermissionResponse::decode(&bytes[..]).map_err(|e| RpcError(e.to_string()))
}

/// PermissionsService.WriteRelationships.
pub fn write_relationships(
    handle: u64,
    request: &WriteRelationshipsRequest,
) -> RpcResult<WriteRelationshipsResponse> {
    let mut buf = Vec::new();
    request
        .encode(&mut buf)
        .map_err(|e| RpcError(e.to_string()))?;
    let bytes = raw_call(handle, &buf, crate::spicedb_permissions_write_relationships)?;
    WriteRelationshipsResponse::decode(&bytes[..]).map_err(|e| RpcError(e.to_string()))
}

/// PermissionsService.DeleteRelationships.
pub fn delete_relationships(
    handle: u64,
    request: &DeleteRelationshipsRequest,
) -> RpcResult<DeleteRelationshipsResponse> {
    let mut buf = Vec::new();
    request
        .encode(&mut buf)
        .map_err(|e| RpcError(e.to_string()))?;
    let bytes = raw_call(
        handle,
        &buf,
        crate::spicedb_permissions_delete_relationships,
    )?;
    DeleteRelationshipsResponse::decode(&bytes[..]).map_err(|e| RpcError(e.to_string()))
}

/// PermissionsService.CheckBulkPermissions.
pub fn check_bulk_permissions(
    handle: u64,
    request: &CheckBulkPermissionsRequest,
) -> RpcResult<CheckBulkPermissionsResponse> {
    let mut buf = Vec::new();
    request
        .encode(&mut buf)
        .map_err(|e| RpcError(e.to_string()))?;
    let bytes = raw_call(
        handle,
        &buf,
        crate::spicedb_permissions_check_bulk_permissions,
    )?;
    CheckBulkPermissionsResponse::decode(&bytes[..]).map_err(|e| RpcError(e.to_string()))
}

/// PermissionsService.ExpandPermissionTree.
pub fn expand_permission_tree(
    handle: u64,
    request: &ExpandPermissionTreeRequest,
) -> RpcResult<ExpandPermissionTreeResponse> {
    let mut buf = Vec::new();
    request
        .encode(&mut buf)
        .map_err(|e| RpcError(e.to_string()))?;
    let bytes = raw_call(
        handle,
        &buf,
        crate::spicedb_permissions_expand_permission_tree,
    )?;
    ExpandPermissionTreeResponse::decode(&bytes[..]).map_err(|e| RpcError(e.to_string()))
}

// --- SchemaService ---

/// SchemaService.ReadSchema.
pub fn read_schema(handle: u64, request: &ReadSchemaRequest) -> RpcResult<ReadSchemaResponse> {
    let mut buf = Vec::new();
    request
        .encode(&mut buf)
        .map_err(|e| RpcError(e.to_string()))?;
    let bytes = raw_call(handle, &buf, crate::spicedb_schema_read_schema)?;
    ReadSchemaResponse::decode(&bytes[..]).map_err(|e| RpcError(e.to_string()))
}

/// SchemaService.WriteSchema.
pub fn write_schema(handle: u64, request: &WriteSchemaRequest) -> RpcResult<WriteSchemaResponse> {
    let mut buf = Vec::new();
    request
        .encode(&mut buf)
        .map_err(|e| RpcError(e.to_string()))?;
    let bytes = raw_call(handle, &buf, crate::spicedb_schema_write_schema)?;
    WriteSchemaResponse::decode(&bytes[..]).map_err(|e| RpcError(e.to_string()))
}
