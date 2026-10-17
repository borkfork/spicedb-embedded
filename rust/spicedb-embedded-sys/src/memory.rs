//! Safe, typed wrappers for the memory-transport FFI.
//!
//! All marshalling/unmarshalling and unsafe FFI calls are contained here.
//! The main crate uses these functions for idiomatic, safe APIs.

use std::{
    ffi::CString,
    os::raw::{c_char, c_int, c_uchar, c_ulonglong},
};

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

/// W3C trace context for a call. SpiceDB's server span joins this trace when `traceparent` is set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraceContext {
    /// The `traceparent` header, e.g. `00-<trace-id>-<span-id>-01`.
    pub traceparent: Option<String>,
    /// The `tracestate` header.
    pub tracestate: Option<String>,
}

/// Full gRPC names of the unary methods [`call`] accepts.
pub mod methods {
    /// PermissionsService.CheckPermission.
    pub const CHECK_PERMISSION: &str = "/authzed.api.v1.PermissionsService/CheckPermission";
    /// PermissionsService.WriteRelationships.
    pub const WRITE_RELATIONSHIPS: &str = "/authzed.api.v1.PermissionsService/WriteRelationships";
    /// PermissionsService.DeleteRelationships.
    pub const DELETE_RELATIONSHIPS: &str = "/authzed.api.v1.PermissionsService/DeleteRelationships";
    /// PermissionsService.CheckBulkPermissions.
    pub const CHECK_BULK_PERMISSIONS: &str =
        "/authzed.api.v1.PermissionsService/CheckBulkPermissions";
    /// PermissionsService.ExpandPermissionTree.
    pub const EXPAND_PERMISSION_TREE: &str =
        "/authzed.api.v1.PermissionsService/ExpandPermissionTree";
    /// SchemaService.ReadSchema.
    pub const READ_SCHEMA: &str = "/authzed.api.v1.SchemaService/ReadSchema";
    /// SchemaService.WriteSchema.
    pub const WRITE_SCHEMA: &str = "/authzed.api.v1.SchemaService/WriteSchema";
}

fn c_string(value: &str) -> RpcResult<CString> {
    CString::new(value).map_err(|_| RpcError(format!("contains a NUL byte: {value:?}")))
}

fn c_string_or_null(value: Option<&String>) -> RpcResult<Option<CString>> {
    value.map(|v| c_string(v)).transpose()
}

/// Invokes a unary method by its full gRPC name (see [`methods`]) under `trace`.
pub fn call<Req: Message, Resp: Message + Default>(
    handle: u64,
    method: &str,
    trace: &TraceContext,
    request: &Req,
) -> RpcResult<Resp> {
    let bytes = raw_call(handle, method, trace, &request.encode_to_vec())?;
    Resp::decode(&bytes[..]).map_err(|e| RpcError(e.to_string()))
}

/// Raw call: request bytes in, response bytes or error string. Handles all unsafe FFI and buffer freeing.
fn raw_call(
    handle: u64,
    method: &str,
    trace: &TraceContext,
    request_bytes: &[u8],
) -> RpcResult<Vec<u8>> {
    let request_len = c_int::try_from(request_bytes.len())
        .map_err(|_| RpcError("request too large for FFI (exceeds c_int::MAX)".into()))?;
    let method = c_string(method)?;
    let traceparent = c_string_or_null(trace.traceparent.as_ref())?;
    let tracestate = c_string_or_null(trace.tracestate.as_ref())?;

    let mut out_response_bytes: *mut c_uchar = std::ptr::null_mut();
    let mut out_response_len: c_int = 0;
    let mut out_error: *mut c_char = std::ptr::null_mut();

    unsafe {
        crate::spicedb_rpc(
            handle as c_ulonglong,
            method.as_ptr(),
            traceparent
                .as_ref()
                .map_or(std::ptr::null(), |s| s.as_ptr()),
            tracestate.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
            request_bytes.as_ptr(),
            request_len,
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

    if out_response_len < 0 {
        return Err(RpcError(format!(
            "FFI returned negative response length: {}",
            out_response_len
        )));
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
    call(
        handle,
        methods::CHECK_PERMISSION,
        &TraceContext::default(),
        request,
    )
}

/// PermissionsService.WriteRelationships.
pub fn write_relationships(
    handle: u64,
    request: &WriteRelationshipsRequest,
) -> RpcResult<WriteRelationshipsResponse> {
    call(
        handle,
        methods::WRITE_RELATIONSHIPS,
        &TraceContext::default(),
        request,
    )
}

/// PermissionsService.DeleteRelationships.
pub fn delete_relationships(
    handle: u64,
    request: &DeleteRelationshipsRequest,
) -> RpcResult<DeleteRelationshipsResponse> {
    call(
        handle,
        methods::DELETE_RELATIONSHIPS,
        &TraceContext::default(),
        request,
    )
}

/// PermissionsService.CheckBulkPermissions.
pub fn check_bulk_permissions(
    handle: u64,
    request: &CheckBulkPermissionsRequest,
) -> RpcResult<CheckBulkPermissionsResponse> {
    call(
        handle,
        methods::CHECK_BULK_PERMISSIONS,
        &TraceContext::default(),
        request,
    )
}

/// PermissionsService.ExpandPermissionTree.
pub fn expand_permission_tree(
    handle: u64,
    request: &ExpandPermissionTreeRequest,
) -> RpcResult<ExpandPermissionTreeResponse> {
    call(
        handle,
        methods::EXPAND_PERMISSION_TREE,
        &TraceContext::default(),
        request,
    )
}

// --- SchemaService ---

/// SchemaService.ReadSchema.
pub fn read_schema(handle: u64, request: &ReadSchemaRequest) -> RpcResult<ReadSchemaResponse> {
    call(
        handle,
        methods::READ_SCHEMA,
        &TraceContext::default(),
        request,
    )
}

/// SchemaService.WriteSchema.
pub fn write_schema(handle: u64, request: &WriteSchemaRequest) -> RpcResult<WriteSchemaResponse> {
    call(
        handle,
        methods::WRITE_SCHEMA,
        &TraceContext::default(),
        request,
    )
}
