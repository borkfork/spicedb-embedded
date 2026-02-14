//! FFI bindings for the `SpiceDB` C-shared library (CGO).
//!
//! This module provides a thin wrapper that starts `SpiceDB` via FFI and
//! connects tonic-generated gRPC clients over a Unix socket (Unix) or TCP (Windows).
//! Schema and relationships are written via gRPC (not JSON).

use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
};

use serde::{Deserialize, Serialize};
// FFI is provided by spicedb-embedded-sys (builds from Go or downloads prebuilt per target).
use spicedb_embedded_sys as native;
use spicedb_grpc::authzed::api::v1::{
    RelationshipUpdate, WriteRelationshipsRequest, WriteSchemaRequest,
    permissions_service_client::PermissionsServiceClient, relationship_update::Operation,
    schema_service_client::SchemaServiceClient, watch_service_client::WatchServiceClient,
};
#[cfg(unix)]
use tokio::net::UnixStream;
#[cfg(unix)]
use tonic::transport::Uri;
use tonic::transport::{Channel, Endpoint};
#[cfg(unix)]
use tower::service_fn;
use tracing::debug;

use crate::SpiceDBError;

/// Response from the C library (JSON parsed)
#[derive(Debug, Deserialize)]
struct CResponse {
    success: bool,
    error: Option<String>,
    data: Option<serde_json::Value>,
}

/// Response from creating a new instance
#[derive(Debug, Deserialize)]
struct NewResponse {
    handle: u64,
    grpc_transport: String,
    address: String,
}

/// Options for starting an embedded `SpiceDB` instance.
#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct StartOptions {
    /// Datastore: "memory" (default), "postgres", "cockroachdb", "spanner", "mysql"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datastore: Option<String>,
    /// Connection string for remote datastores. Required for postgres, cockroachdb, spanner, mysql.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datastore_uri: Option<String>,
    /// gRPC transport: "unix" (default on Unix), "tcp" (default on Windows)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grpc_transport: Option<String>,
    /// Path to Spanner service account JSON (Spanner only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spanner_credentials_file: Option<String>,
    /// Spanner emulator host, e.g. "localhost:9010" (Spanner only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spanner_emulator_host: Option<String>,
    /// Prefix for all tables (`MySQL` only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mysql_table_prefix: Option<String>,
    /// Enable datastore Prometheus metrics (default: false; disabled allows multiple instances in same process)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics_enabled: Option<bool>,
}

/// Helper to call a C function and parse the JSON response
///
/// # Safety
///
/// The caller must ensure `ptr` is a valid pointer returned by one of the
/// `spicedb_*` functions, or null.
unsafe fn call_and_parse(ptr: *mut c_char) -> Result<serde_json::Value, SpiceDBError> {
    if ptr.is_null() {
        return Err(SpiceDBError::Runtime("null response from C library".into()));
    }

    let c_str = unsafe { CStr::from_ptr(ptr) };
    let response_str = c_str.to_string_lossy().into_owned();
    unsafe { native::spicedb_free(ptr) };

    debug!("FFI response: {}", response_str);

    let response: CResponse = serde_json::from_str(&response_str)
        .map_err(|e| SpiceDBError::Protocol(format!("invalid JSON: {e} (raw: {response_str})")))?;

    if response.success {
        Ok(response.data.unwrap_or(serde_json::Value::Null))
    } else {
        Err(SpiceDBError::SpiceDB(
            response.error.unwrap_or_else(|| "unknown error".into()),
        ))
    }
}

/// Embedded `SpiceDB` instance.
///
/// A thin wrapper that connects auto-generated tonic gRPC clients over a Unix
/// socket. Use [`permissions`](EmbeddedSpiceDB::permissions), [`schema`](EmbeddedSpiceDB::schema),
/// and [`watch`](EmbeddedSpiceDB::watch) to access the full `SpiceDB` API.
pub struct EmbeddedSpiceDB {
    handle: u64,
    channel: Channel,
}

unsafe impl Send for EmbeddedSpiceDB {}
unsafe impl Sync for EmbeddedSpiceDB {}

impl EmbeddedSpiceDB {
    /// Create a new embedded `SpiceDB` instance with a schema and relationships.
    ///
    /// This starts a `SpiceDB` server via the C-shared library, connects over a Unix
    /// socket or TCP, then bootstraps schema and relationships via gRPC.
    ///
    /// # Arguments
    ///
    /// * `schema` - The `SpiceDB` schema definition (ZED language)
    /// * `relationships` - Initial relationships (uses types from `spicedb_grpc::authzed::api::v1`)
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to start, connection fails, or gRPC writes fail.
    pub async fn new(
        schema: &str,
        relationships: &[spicedb_grpc::authzed::api::v1::Relationship],
    ) -> Result<Self, SpiceDBError> {
        Self::new_with_options(schema, relationships, None).await
    }

    /// Create a new embedded `SpiceDB` instance with options.
    ///
    /// This starts a `SpiceDB` server via the C-shared library, connects over a Unix
    /// socket or TCP, then bootstraps schema and relationships via gRPC.
    ///
    /// # Arguments
    ///
    /// * `schema` - The `SpiceDB` schema definition (ZED language)
    /// * `relationships` - Initial relationships (uses types from `spicedb_grpc::authzed::api::v1`)
    /// * `options` - Optional configuration (`datastore`, `grpc_transport`). Use `None` for defaults.
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to start, connection fails, or gRPC writes fail.
    pub async fn new_with_options(
        schema: &str,
        relationships: &[spicedb_grpc::authzed::api::v1::Relationship],
        options: Option<&StartOptions>,
    ) -> Result<Self, SpiceDBError> {
        debug!(
            "Starting SpiceDB instance with schema ({} bytes), {} relationships",
            schema.len(),
            relationships.len()
        );

        let (options_ptr, _cstr_guard): (*const c_char, Option<CString>) = match options {
            Some(opts) => {
                let json = serde_json::to_string(opts).map_err(|e| {
                    SpiceDBError::Protocol(format!("failed to serialize options: {e}"))
                })?;
                let cstr = CString::new(json).map_err(|e| {
                    SpiceDBError::Protocol(format!("options contains null byte: {e}"))
                })?;
                let ptr = cstr.as_ptr();
                (ptr, Some(cstr))
            }
            None => (std::ptr::null(), None),
        };

        let data = unsafe {
            let result = native::spicedb_start(options_ptr);
            call_and_parse(result)?
        };

        let new_resp: NewResponse = serde_json::from_value(data)
            .map_err(|e| SpiceDBError::Protocol(format!("invalid new response: {e}")))?;

        debug!(
            "Connecting to SpiceDB at {} ({})",
            new_resp.address, new_resp.grpc_transport
        );

        let channel = connect_to_spicedb(&new_resp.grpc_transport, &new_resp.address)
            .await
            .map_err(|e| {
                unsafe {
                    let result = native::spicedb_dispose(new_resp.handle);
                    if !result.is_null() {
                        native::spicedb_free(result);
                    }
                }
                SpiceDBError::Runtime(format!("failed to connect to SpiceDB: {e}"))
            })?;

        let db = Self {
            handle: new_resp.handle,
            channel,
        };

        // Bootstrap via gRPC
        db.schema()
            .write_schema(tonic::Request::new(WriteSchemaRequest {
                schema: schema.to_string(),
            }))
            .await
            .map_err(|e| SpiceDBError::SpiceDB(format!("write schema failed: {e}")))?;

        if !relationships.is_empty() {
            let updates: Vec<RelationshipUpdate> = relationships
                .iter()
                .map(|rel| RelationshipUpdate {
                    operation: Operation::Touch as i32,
                    relationship: Some(rel.clone()),
                })
                .collect();
            db.permissions()
                .write_relationships(tonic::Request::new(WriteRelationshipsRequest {
                    updates,
                    optional_preconditions: vec![],
                }))
                .await
                .map_err(|e| SpiceDBError::SpiceDB(format!("write relationships failed: {e}")))?;
        }

        Ok(db)
    }

    /// Permissions service client (`CheckPermission`, `WriteRelationships`, `ReadRelationships`, etc.)
    #[must_use]
    pub fn permissions(&self) -> PermissionsServiceClient<Channel> {
        PermissionsServiceClient::new(self.channel.clone())
    }

    /// Schema service client (`ReadSchema`, `WriteSchema`, `ReflectSchema`, etc.)
    #[must_use]
    pub fn schema(&self) -> SchemaServiceClient<Channel> {
        SchemaServiceClient::new(self.channel.clone())
    }

    /// Watch service client (watch for relationship changes)
    #[must_use]
    pub fn watch(&self) -> WatchServiceClient<Channel> {
        WatchServiceClient::new(self.channel.clone())
    }
}

impl Drop for EmbeddedSpiceDB {
    fn drop(&mut self) {
        debug!("Disposing SpiceDB handle {}", self.handle);
        unsafe {
            let result = native::spicedb_dispose(self.handle);
            if !result.is_null() {
                native::spicedb_free(result);
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Memory transport: idiomatic API using -sys safe layer (no unsafe in this crate)
// -----------------------------------------------------------------------------

use spicedb_embedded_sys::memory_transport;
use spicedb_grpc::authzed::api::v1::{
    CheckBulkPermissionsRequest, CheckBulkPermissionsResponse, CheckPermissionRequest,
    CheckPermissionResponse, DeleteRelationshipsRequest, DeleteRelationshipsResponse,
    ExpandPermissionTreeRequest, ExpandPermissionTreeResponse, ReadSchemaRequest,
    ReadSchemaResponse, WriteRelationshipsResponse, WriteSchemaResponse,
};

/// Embedded SpiceDB using in-memory transport (no socket). All RPCs go through the FFI.
/// Use [`permissions`](MemorySpiceDB::permissions) and [`schema`](MemorySpiceDB::schema) for typed APIs.
pub struct MemorySpiceDB {
    handle: u64,
}

unsafe impl Send for MemorySpiceDB {}
unsafe impl Sync for MemorySpiceDB {}

impl MemorySpiceDB {
    /// Start a SpiceDB instance with `grpc_transport: "memory"` and optional bootstrap.
    /// If `schema` is non-empty, writes it via SchemaService. If `relationships` is non-empty, writes them via WriteRelationships.
    pub fn new(
        schema: &str,
        relationships: &[spicedb_grpc::authzed::api::v1::Relationship],
        options: Option<&StartOptions>,
    ) -> Result<Self, SpiceDBError> {
        let opts = options.cloned().unwrap_or_default();
        let mut opts = opts;
        opts.grpc_transport = Some("memory".into());
        let json = serde_json::to_string(&opts)
            .map_err(|e| SpiceDBError::Protocol(format!("serialize options: {e}")))?;
        let cstr = std::ffi::CString::new(json)
            .map_err(|e| SpiceDBError::Protocol(format!("options null byte: {e}")))?;
        let ptr = unsafe { native::spicedb_start(cstr.as_ptr()) };
        let data = unsafe { call_and_parse(ptr)? };
        let handle = data
            .get("handle")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| SpiceDBError::Protocol("missing handle in start response".into()))?;
        let grpc_transport = data
            .get("grpc_transport")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SpiceDBError::Protocol("missing grpc_transport".into()))?;
        if grpc_transport != "memory" {
            return Err(SpiceDBError::Protocol(format!(
                "expected grpc_transport memory, got {grpc_transport}"
            )));
        }

        let db = Self { handle };

        if !schema.is_empty() {
            memory_transport::write_schema(
                db.handle,
                &WriteSchemaRequest {
                    schema: schema.to_string(),
                },
            )
            .map_err(|e| SpiceDBError::SpiceDB(e.0))?;
        }

        if !relationships.is_empty() {
            let updates: Vec<RelationshipUpdate> = relationships
                .iter()
                .map(|r| RelationshipUpdate {
                    operation: Operation::Touch as i32,
                    relationship: Some(r.clone()),
                })
                .collect();
            memory_transport::write_relationships(
                db.handle,
                &WriteRelationshipsRequest {
                    updates,
                    optional_preconditions: vec![],
                },
            )
            .map_err(|e| SpiceDBError::SpiceDB(e.0))?;
        }

        Ok(db)
    }

    /// Permissions service (CheckPermission, WriteRelationships, DeleteRelationships, etc.).
    #[must_use]
    pub fn permissions(&self) -> MemoryPermissionsClient {
        MemoryPermissionsClient {
            handle: self.handle,
        }
    }

    /// Schema service (ReadSchema, WriteSchema).
    #[must_use]
    pub fn schema(&self) -> MemorySchemaClient {
        MemorySchemaClient {
            handle: self.handle,
        }
    }

    /// Raw handle for advanced use (e.g. with [`spicedb_embedded_sys::memory_transport`]).
    #[must_use]
    pub fn handle(&self) -> u64 {
        self.handle
    }
}

impl Drop for MemorySpiceDB {
    fn drop(&mut self) {
        debug!("Disposing memory SpiceDB handle {}", self.handle);
        unsafe {
            let result = native::spicedb_dispose(self.handle);
            if !result.is_null() {
                native::spicedb_free(result);
            }
        }
    }
}

/// Permissions service client for memory transport. All methods are synchronous and use the -sys safe layer.
pub struct MemoryPermissionsClient {
    handle: u64,
}

impl MemoryPermissionsClient {
    /// CheckPermission.
    pub fn check_permission(
        &self,
        request: &CheckPermissionRequest,
    ) -> Result<CheckPermissionResponse, SpiceDBError> {
        memory_transport::check_permission(self.handle, request).map_err(|e| SpiceDBError::SpiceDB(e.0))
    }

    /// WriteRelationships.
    pub fn write_relationships(
        &self,
        request: &WriteRelationshipsRequest,
    ) -> Result<WriteRelationshipsResponse, SpiceDBError> {
        memory_transport::write_relationships(self.handle, request).map_err(|e| SpiceDBError::SpiceDB(e.0))
    }

    /// DeleteRelationships.
    pub fn delete_relationships(
        &self,
        request: &DeleteRelationshipsRequest,
    ) -> Result<DeleteRelationshipsResponse, SpiceDBError> {
        memory_transport::delete_relationships(self.handle, request).map_err(|e| SpiceDBError::SpiceDB(e.0))
    }

    /// CheckBulkPermissions.
    pub fn check_bulk_permissions(
        &self,
        request: &CheckBulkPermissionsRequest,
    ) -> Result<CheckBulkPermissionsResponse, SpiceDBError> {
        memory_transport::check_bulk_permissions(self.handle, request)
            .map_err(|e| SpiceDBError::SpiceDB(e.0))
    }

    /// ExpandPermissionTree.
    pub fn expand_permission_tree(
        &self,
        request: &ExpandPermissionTreeRequest,
    ) -> Result<ExpandPermissionTreeResponse, SpiceDBError> {
        memory_transport::expand_permission_tree(self.handle, request)
            .map_err(|e| SpiceDBError::SpiceDB(e.0))
    }
}

/// Schema service client for memory transport.
pub struct MemorySchemaClient {
    handle: u64,
}

impl MemorySchemaClient {
    /// ReadSchema.
    pub fn read_schema(
        &self,
        request: &ReadSchemaRequest,
    ) -> Result<ReadSchemaResponse, SpiceDBError> {
        memory_transport::read_schema(self.handle, request).map_err(|e| SpiceDBError::SpiceDB(e.0))
    }

    /// WriteSchema.
    pub fn write_schema(
        &self,
        request: &WriteSchemaRequest,
    ) -> Result<WriteSchemaResponse, SpiceDBError> {
        memory_transport::write_schema(self.handle, request).map_err(|e| SpiceDBError::SpiceDB(e.0))
    }
}

async fn connect_to_spicedb(
    transport: &str,
    address: &str,
) -> Result<Channel, Box<dyn std::error::Error + Send + Sync>> {
    if transport == "tcp" {
        connect_tcp(address).await
    } else {
        connect_unix_socket(address).await
    }
}

async fn connect_tcp(address: &str) -> Result<Channel, Box<dyn std::error::Error + Send + Sync>> {
    Endpoint::from_shared(format!("http://{address}"))?
        .connect()
        .await
        .map_err(Into::into)
}

#[cfg(unix)]
async fn connect_unix_socket(
    address: &str,
) -> Result<Channel, Box<dyn std::error::Error + Send + Sync>> {
    let path = address.to_string();
    let channel = Endpoint::try_from("http://[::]:50051")?
        .connect_with_connector(service_fn(move |_: Uri| {
            let path = path.clone();
            async move {
                let stream = UnixStream::connect(&path).await?;
                Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(stream))
            }
        }))
        .await?;
    Ok(channel)
}

#[cfg(windows)]
fn connect_unix_socket(
    _address: &str,
) -> std::future::Ready<Result<Channel, Box<dyn std::error::Error + Send + Sync>>> {
    std::future::ready(Err(
        "Unix domain sockets are not supported on Windows".into()
    ))
}

#[cfg(test)]
mod tests {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int, c_uchar, c_ulonglong};

    use prost::Message;
    use spicedb_grpc::authzed::api::v1::{
        CheckPermissionRequest, CheckPermissionResponse, Consistency, ObjectReference,
        ReadRelationshipsRequest, Relationship, RelationshipFilter, RelationshipUpdate,
        SubjectReference, WriteRelationshipsRequest, WriteSchemaRequest,
        relationship_update::Operation,
    };
    use tokio_stream::StreamExt;

    use super::*;
    use crate::v1::check_permission_response::Permissionship;

    const TEST_SCHEMA: &str = r"
definition user {}

definition document {
    relation reader: user
    relation writer: user

    permission read = reader + writer
    permission write = writer
}
";

    fn rel(resource: &str, relation: &str, subject: &str) -> Relationship {
        let (res_type, res_id) = resource.split_once(':').unwrap();
        let (sub_type, sub_id) = subject.split_once(':').unwrap();
        Relationship {
            resource: Some(ObjectReference {
                object_type: res_type.into(),
                object_id: res_id.into(),
            }),
            relation: relation.into(),
            subject: Some(SubjectReference {
                object: Some(ObjectReference {
                    object_type: sub_type.into(),
                    object_id: sub_id.into(),
                }),
                optional_relation: String::new(),
            }),
            optional_caveat: None,
        }
    }

    fn fully_consistent() -> Consistency {
        Consistency {
            requirement: Some(crate::v1::consistency::Requirement::FullyConsistent(true)),
        }
    }

    fn check_req(resource: &str, permission: &str, subject: &str) -> CheckPermissionRequest {
        let (res_type, res_id) = resource.split_once(':').unwrap();
        let (sub_type, sub_id) = subject.split_once(':').unwrap();
        CheckPermissionRequest {
            consistency: Some(fully_consistent()),
            resource: Some(ObjectReference {
                object_type: res_type.into(),
                object_id: res_id.into(),
            }),
            permission: permission.into(),
            subject: Some(SubjectReference {
                object: Some(ObjectReference {
                    object_type: sub_type.into(),
                    object_id: sub_id.into(),
                }),
                optional_relation: String::new(),
            }),
            context: None,
            with_tracing: false,
        }
    }

    /// Calls an RPC FFI (check_permission, write_schema, write_relationships). Returns response bytes or error string.
    unsafe fn call_memory_rpc(
        handle: u64,
        request_bytes: &[u8],
        rpc_fn: unsafe extern "C" fn(
            c_ulonglong,
            *const c_uchar,
            c_int,
            *mut *mut c_uchar,
            *mut c_int,
            *mut *mut c_char,
        ),
    ) -> Result<Vec<u8>, String> {
        let mut out_response_bytes: *mut c_uchar = std::ptr::null_mut();
        let mut out_response_len: c_int = 0;
        let mut out_error: *mut c_char = std::ptr::null_mut();
        unsafe {
            rpc_fn(
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
            unsafe { native::spicedb_free(out_error) };
            return Err(s);
        }
        let len = out_response_len as usize;
        let bytes = if len == 0 {
            vec![]
        } else {
            unsafe { std::slice::from_raw_parts(out_response_bytes, len).to_vec() }
        };
        if !out_response_bytes.is_null() {
            unsafe { native::spicedb_free_bytes(out_response_bytes as *mut std::ffi::c_void) };
        }
        Ok(bytes)
    }

    /// Starts a SpiceDB instance with grpc_transport "memory" via raw FFI; returns (handle, _guard to free start result).
    fn start_memory_server() -> Result<(u64, CString), SpiceDBError> {
        let opts = StartOptions {
            grpc_transport: Some("memory".into()),
            ..Default::default()
        };
        let json = serde_json::to_string(&opts).map_err(|e| SpiceDBError::Protocol(format!("serialize options: {e}")))?;
        let cstr = CString::new(json).map_err(|e| SpiceDBError::Protocol(format!("options null byte: {e}")))?;
        let ptr = unsafe { native::spicedb_start(cstr.as_ptr()) };
        let data = unsafe { call_and_parse(ptr)? };
        let handle = data
            .get("handle")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| SpiceDBError::Protocol("missing handle in start response".into()))?;
        let grpc_transport = data
            .get("grpc_transport")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SpiceDBError::Protocol("missing grpc_transport in start response".into()))?;
        if grpc_transport != "memory" {
            return Err(SpiceDBError::Protocol(format!("expected grpc_transport memory, got {grpc_transport}")));
        }
        Ok((handle, cstr))
    }

    /// Idiomatic memory transport: MemorySpiceDB::new + .permissions().check_permission() (uses -sys safe layer).
    #[tokio::test]
    async fn test_memory_transport_check_permission() {
        let relationships = vec![
            rel("document:readme", "reader", "user:alice"),
            rel("document:readme", "writer", "user:bob"),
        ];

        let spicedb = MemorySpiceDB::new(TEST_SCHEMA, &relationships, None).unwrap();

        let response = spicedb
            .permissions()
            .check_permission(&check_req("document:readme", "read", "user:alice"))
            .unwrap();
        assert_eq!(
            response.permissionship,
            Permissionship::HasPermission as i32,
            "alice should have read on document:readme"
        );
    }

    #[tokio::test]
    async fn test_ffi_spicedb() {
        let relationships = vec![
            rel("document:readme", "reader", "user:alice"),
            rel("document:readme", "writer", "user:bob"),
        ];

        let spicedb = EmbeddedSpiceDB::new(TEST_SCHEMA, &relationships)
            .await
            .unwrap();

        assert!(
            spicedb
                .permissions()
                .check_permission(tonic::Request::new(check_req(
                    "document:readme",
                    "read",
                    "user:alice"
                )))
                .await
                .unwrap()
                .into_inner()
                .permissionship
                == Permissionship::HasPermission as i32
        );
        assert_eq!(
            spicedb
                .permissions()
                .check_permission(tonic::Request::new(check_req(
                    "document:readme",
                    "write",
                    "user:alice"
                )))
                .await
                .unwrap()
                .into_inner()
                .permissionship,
            Permissionship::NoPermission as i32,
        );
        assert!(
            spicedb
                .permissions()
                .check_permission(tonic::Request::new(check_req(
                    "document:readme",
                    "read",
                    "user:bob"
                )))
                .await
                .unwrap()
                .into_inner()
                .permissionship
                == Permissionship::HasPermission as i32
        );
        assert!(
            spicedb
                .permissions()
                .check_permission(tonic::Request::new(check_req(
                    "document:readme",
                    "write",
                    "user:bob"
                )))
                .await
                .unwrap()
                .into_inner()
                .permissionship
                == Permissionship::HasPermission as i32
        );
        assert_eq!(
            spicedb
                .permissions()
                .check_permission(tonic::Request::new(check_req(
                    "document:readme",
                    "read",
                    "user:charlie"
                )))
                .await
                .unwrap()
                .into_inner()
                .permissionship,
            Permissionship::NoPermission as i32,
        );
    }

    #[tokio::test]
    async fn test_add_relationship() {
        let spicedb = EmbeddedSpiceDB::new(TEST_SCHEMA, &[]).await.unwrap();

        spicedb
            .permissions()
            .write_relationships(tonic::Request::new(WriteRelationshipsRequest {
                updates: vec![RelationshipUpdate {
                    operation: Operation::Touch as i32,
                    relationship: Some(rel("document:test", "reader", "user:alice")),
                }],
                optional_preconditions: vec![],
            }))
            .await
            .unwrap();

        let r = spicedb
            .permissions()
            .check_permission(tonic::Request::new(CheckPermissionRequest {
                consistency: Some(fully_consistent()),
                resource: Some(ObjectReference {
                    object_type: "document".into(),
                    object_id: "test".into(),
                }),
                permission: "read".into(),
                subject: Some(SubjectReference {
                    object: Some(ObjectReference {
                        object_type: "user".into(),
                        object_id: "alice".into(),
                    }),
                    optional_relation: String::new(),
                }),
                context: None,
                with_tracing: false,
            }))
            .await
            .unwrap();
        assert_eq!(
            r.into_inner().permissionship,
            Permissionship::HasPermission as i32
        );
    }

    #[tokio::test]
    async fn test_parallel_instances() {
        let spicedb1 = EmbeddedSpiceDB::new(TEST_SCHEMA, &[]).await.unwrap();
        let spicedb2 = EmbeddedSpiceDB::new(TEST_SCHEMA, &[]).await.unwrap();

        spicedb1
            .permissions()
            .write_relationships(tonic::Request::new(WriteRelationshipsRequest {
                updates: vec![RelationshipUpdate {
                    operation: Operation::Touch as i32,
                    relationship: Some(rel("document:doc1", "reader", "user:alice")),
                }],
                optional_preconditions: vec![],
            }))
            .await
            .unwrap();

        let r1 = spicedb1
            .permissions()
            .check_permission(tonic::Request::new(CheckPermissionRequest {
                consistency: Some(fully_consistent()),
                resource: Some(ObjectReference {
                    object_type: "document".into(),
                    object_id: "doc1".into(),
                }),
                permission: "read".into(),
                subject: Some(SubjectReference {
                    object: Some(ObjectReference {
                        object_type: "user".into(),
                        object_id: "alice".into(),
                    }),
                    optional_relation: String::new(),
                }),
                context: None,
                with_tracing: false,
            }))
            .await
            .unwrap();
        assert_eq!(
            r1.into_inner().permissionship,
            Permissionship::HasPermission as i32
        );

        let r2 = spicedb2
            .permissions()
            .check_permission(tonic::Request::new(CheckPermissionRequest {
                consistency: Some(fully_consistent()),
                resource: Some(ObjectReference {
                    object_type: "document".into(),
                    object_id: "doc1".into(),
                }),
                permission: "read".into(),
                subject: Some(SubjectReference {
                    object: Some(ObjectReference {
                        object_type: "user".into(),
                        object_id: "alice".into(),
                    }),
                    optional_relation: String::new(),
                }),
                context: None,
                with_tracing: false,
            }))
            .await
            .unwrap();
        assert_eq!(
            r2.into_inner().permissionship,
            Permissionship::NoPermission as i32
        );
    }

    #[tokio::test]
    async fn test_read_relationships() {
        let relationships = vec![
            rel("document:doc1", "reader", "user:alice"),
            rel("document:doc1", "reader", "user:bob"),
        ];

        let spicedb = EmbeddedSpiceDB::new(TEST_SCHEMA, &relationships)
            .await
            .unwrap();

        let mut client = spicedb.permissions();
        let mut stream = client
            .read_relationships(tonic::Request::new(ReadRelationshipsRequest {
                consistency: Some(fully_consistent()),
                relationship_filter: Some(RelationshipFilter {
                    resource_type: "document".into(),
                    optional_resource_id: "doc1".into(),
                    optional_resource_id_prefix: String::new(),
                    optional_relation: String::new(),
                    optional_subject_filter: None,
                }),
                optional_limit: 0,
                optional_cursor: None,
            }))
            .await
            .unwrap()
            .into_inner();

        let mut count = 0;
        while let Some(Ok(_)) = stream.next().await {
            count += 1;
        }
        assert_eq!(count, 2);
    }

    /// Performance test - run with `cargo test perf_ -- --nocapture --ignored`
    #[tokio::test]
    #[ignore = "performance test - run manually with --ignored flag"]
    async fn perf_check_with_1000_relationships() {
        const NUM_CHECKS: usize = 100;
        use std::time::Instant;

        // Create 1000 relationships
        let relationships: Vec<Relationship> = (0..1000)
            .map(|i| {
                rel(
                    &format!("document:doc{i}"),
                    "reader",
                    &format!("user:user{}", i % 100),
                )
            })
            .collect();

        println!("\n=== Performance: Check with 1000 relationships ===");

        let start = Instant::now();
        let spicedb = EmbeddedSpiceDB::new(TEST_SCHEMA, &relationships)
            .await
            .unwrap();
        println!(
            "Instance creation with 1000 relationships: {:?}",
            start.elapsed()
        );

        // Warm up
        for _ in 0..10 {
            let _ = spicedb
                .permissions()
                .check_permission(tonic::Request::new(check_req(
                    "document:doc0",
                    "read",
                    "user:user0",
                )))
                .await;
        }

        // Benchmark permission checks
        let start = Instant::now();
        for i in 0..NUM_CHECKS {
            let doc = format!("document:doc{}", i % 1000);
            let user = format!("user:user{}", i % 100);
            let _ = spicedb
                .permissions()
                .check_permission(tonic::Request::new(check_req(&doc, "read", &user)))
                .await
                .unwrap();
        }
        let elapsed = start.elapsed();

        let num_checks_u32 = u32::try_from(NUM_CHECKS).unwrap();
        println!("Total time for {NUM_CHECKS} checks: {elapsed:?}");
        println!("Average per check: {:?}", elapsed / num_checks_u32);
        println!(
            "Checks per second: {:.0}",
            f64::from(num_checks_u32) / elapsed.as_secs_f64()
        );
    }

    /// Performance test for individual relationship additions
    #[tokio::test]
    #[ignore = "performance test - run manually with --ignored flag"]
    async fn perf_add_individual_relationships() {
        const NUM_ADDS: usize = 50;
        use std::time::Instant;

        let spicedb = EmbeddedSpiceDB::new(TEST_SCHEMA, &[]).await.unwrap();

        println!("\n=== Performance: Add individual relationships ===");

        let start = Instant::now();
        for i in 0..NUM_ADDS {
            spicedb
                .permissions()
                .write_relationships(tonic::Request::new(WriteRelationshipsRequest {
                    updates: vec![RelationshipUpdate {
                        operation: Operation::Touch as i32,
                        relationship: Some(rel(
                            &format!("document:perf{i}"),
                            "reader",
                            "user:alice",
                        )),
                    }],
                    optional_preconditions: vec![],
                }))
                .await
                .unwrap();
        }
        let elapsed = start.elapsed();

        let num_adds_u32 = u32::try_from(NUM_ADDS).unwrap();
        println!("Total time for {NUM_ADDS} individual adds: {elapsed:?}");
        println!("Average per add: {:?}", elapsed / num_adds_u32);
        println!(
            "Adds per second: {:.0}",
            f64::from(num_adds_u32) / elapsed.as_secs_f64()
        );
    }

    /// Performance test for bulk relationship writes
    #[tokio::test]
    #[ignore = "performance test - run manually with --ignored flag"]
    async fn perf_bulk_write_relationships() {
        use std::time::Instant;

        let spicedb = EmbeddedSpiceDB::new(TEST_SCHEMA, &[]).await.unwrap();

        println!("\n=== Performance: Bulk write relationships ===");

        // Test different batch sizes
        for batch_size in [5_i32, 10, 20, 50] {
            let batch_size_u32 = u32::try_from(batch_size).unwrap();
            let relationships: Vec<Relationship> = (0..batch_size)
                .map(|i| {
                    rel(
                        &format!("document:bulk{batch_size}_{i}"),
                        "reader",
                        "user:alice",
                    )
                })
                .collect();

            let start = Instant::now();
            spicedb
                .permissions()
                .write_relationships(tonic::Request::new(WriteRelationshipsRequest {
                    updates: relationships
                        .iter()
                        .map(|r| RelationshipUpdate {
                            operation: Operation::Touch as i32,
                            relationship: Some(r.clone()),
                        })
                        .collect(),
                    optional_preconditions: vec![],
                }))
                .await
                .unwrap();
            let elapsed = start.elapsed();

            println!(
                "Batch of {} relationships: {:?} ({:?} per relationship)",
                batch_size,
                elapsed,
                elapsed / batch_size_u32
            );
        }

        // Compare: 10 individual vs 10 bulk
        println!("\n--- Comparison: 10 individual vs 10 bulk ---");

        let spicedb2 = EmbeddedSpiceDB::new(TEST_SCHEMA, &[]).await.unwrap();

        let start = Instant::now();
        for i in 0..10 {
            spicedb2
                .permissions()
                .write_relationships(tonic::Request::new(WriteRelationshipsRequest {
                    updates: vec![RelationshipUpdate {
                        operation: Operation::Touch as i32,
                        relationship: Some(rel(
                            &format!("document:cmp_ind{i}"),
                            "reader",
                            "user:bob",
                        )),
                    }],
                    optional_preconditions: vec![],
                }))
                .await
                .unwrap();
        }
        let individual_time = start.elapsed();
        println!("10 individual adds: {individual_time:?}");

        let relationships: Vec<Relationship> = (0..10)
            .map(|i| rel(&format!("document:cmp_bulk{i}"), "reader", "user:bob"))
            .collect();
        let start = Instant::now();
        spicedb2
            .permissions()
            .write_relationships(tonic::Request::new(WriteRelationshipsRequest {
                updates: relationships
                    .iter()
                    .map(|r| RelationshipUpdate {
                        operation: Operation::Touch as i32,
                        relationship: Some(r.clone()),
                    })
                    .collect(),
                optional_preconditions: vec![],
            }))
            .await
            .unwrap();
        let bulk_time = start.elapsed();
        println!("10 bulk add: {bulk_time:?}");
        println!(
            "Speedup: {:.1}x",
            individual_time.as_secs_f64() / bulk_time.as_secs_f64()
        );
    }

    /// Performance test with 50,000 relationships - Embedded
    #[tokio::test]
    #[ignore = "performance test - run manually with --ignored flag"]
    async fn perf_embedded_50k_relationships() {
        const TOTAL_RELS: usize = 50_000;
        const BATCH_SIZE: usize = 1000;
        const NUM_CHECKS: usize = 500;
        use std::time::Instant;

        println!("\n=== Embedded SpiceDB: 50,000 relationships ===");

        // Create 50,000 relationships in batches (SpiceDB max batch is 1000)
        println!("Creating instance...");
        let start = Instant::now();
        let spicedb = EmbeddedSpiceDB::new(TEST_SCHEMA, &[]).await.unwrap();
        println!("Instance creation time: {:?}", start.elapsed());

        println!("Adding {TOTAL_RELS} relationships in batches of {BATCH_SIZE}...");
        let start = Instant::now();
        for batch_num in 0..(TOTAL_RELS / BATCH_SIZE) {
            let batch_start = batch_num * BATCH_SIZE;
            let relationships: Vec<Relationship> = (batch_start..batch_start + BATCH_SIZE)
                .map(|i| {
                    rel(
                        &format!("document:doc{i}"),
                        "reader",
                        &format!("user:user{}", i % 1000),
                    )
                })
                .collect();
            spicedb
                .permissions()
                .write_relationships(tonic::Request::new(WriteRelationshipsRequest {
                    updates: relationships
                        .iter()
                        .map(|r| RelationshipUpdate {
                            operation: Operation::Touch as i32,
                            relationship: Some(r.clone()),
                        })
                        .collect(),
                    optional_preconditions: vec![],
                }))
                .await
                .unwrap();
        }
        println!(
            "Total time to add {} relationships: {:?}",
            TOTAL_RELS,
            start.elapsed()
        );

        // Warm up
        for _ in 0..20 {
            let _ = spicedb
                .permissions()
                .check_permission(tonic::Request::new(check_req(
                    "document:doc0",
                    "read",
                    "user:user0",
                )))
                .await;
        }

        // Benchmark permission checks
        let num_checks_u32 = u32::try_from(NUM_CHECKS).unwrap();
        let start = Instant::now();
        for i in 0..NUM_CHECKS {
            let doc = format!("document:doc{}", i % TOTAL_RELS);
            let user = format!("user:user{}", i % 1000);
            let _ = spicedb
                .permissions()
                .check_permission(tonic::Request::new(check_req(&doc, "read", &user)))
                .await
                .unwrap();
        }
        let elapsed = start.elapsed();

        println!("Total time for {NUM_CHECKS} checks: {elapsed:?}");
        println!("Average per check: {:?}", elapsed / num_checks_u32);
        println!(
            "Checks per second: {:.0}",
            f64::from(num_checks_u32) / elapsed.as_secs_f64()
        );

        // Test some negative checks too
        let start = Instant::now();
        for i in 0..NUM_CHECKS {
            let doc = format!("document:doc{}", i % TOTAL_RELS);
            // user:nonexistent doesn't exist
            let _ = spicedb
                .permissions()
                .check_permission(tonic::Request::new(check_req(
                    &doc,
                    "read",
                    "user:nonexistent",
                )))
                .await
                .unwrap();
        }
        let elapsed = start.elapsed();
        println!("\nNegative checks (user not found):");
        println!("Average per check: {:?}", elapsed / num_checks_u32);
    }

    // -------------------------------------------------------------------------
    // Memory-transport performance tests (same scenarios as above, via FFI)
    // Run with: cargo test perf_memory_ -- --nocapture --ignored
    // -------------------------------------------------------------------------

    /// Performance test (memory transport): Check with 1000 relationships.
    #[tokio::test]
    #[ignore = "performance test - run manually with --ignored flag"]
    async fn perf_memory_check_with_1000_relationships() {
        const NUM_CHECKS: usize = 100;
        use std::time::Instant;

        let (handle, _guard) = start_memory_server().unwrap();

        // Bootstrap schema
        let mut schema_bytes = Vec::new();
        WriteSchemaRequest {
            schema: TEST_SCHEMA.to_string(),
        }
        .encode(&mut schema_bytes)
        .unwrap();
        unsafe {
            call_memory_rpc(handle, &schema_bytes, native::spicedb_schema_write_schema)
        }
        .unwrap();

        // Create 1000 relationships
        let relationships: Vec<Relationship> = (0..1000)
            .map(|i| {
                rel(
                    &format!("document:doc{i}"),
                    "reader",
                    &format!("user:user{}", i % 100),
                )
            })
            .collect();
        let updates: Vec<RelationshipUpdate> = relationships
            .iter()
            .map(|r| RelationshipUpdate {
                operation: Operation::Touch as i32,
                relationship: Some(r.clone()),
            })
            .collect();
        let mut rel_bytes = Vec::new();
        WriteRelationshipsRequest {
            updates,
            optional_preconditions: vec![],
        }
        .encode(&mut rel_bytes)
        .unwrap();

        println!("\n=== Performance (memory transport): Check with 1000 relationships ===");

        let start = Instant::now();
        unsafe {
            call_memory_rpc(handle, &rel_bytes, native::spicedb_permissions_write_relationships)
        }
        .unwrap();
        println!("Write 1000 relationships: {:?}", start.elapsed());

        // Warm up
        let warm = check_req("document:doc0", "read", "user:user0");
        let mut warm_bytes = Vec::new();
        warm.encode(&mut warm_bytes).unwrap();
        for _ in 0..10 {
            let _ = unsafe {
                call_memory_rpc(handle, &warm_bytes, native::spicedb_permissions_check_permission)
            };
        }

        // Benchmark permission checks
        let start = Instant::now();
        for i in 0..NUM_CHECKS {
            let req = check_req(
                &format!("document:doc{}", i % 1000),
                "read",
                &format!("user:user{}", i % 100),
            );
            let mut buf = Vec::new();
            req.encode(&mut buf).unwrap();
            let resp = unsafe {
                call_memory_rpc(handle, &buf, native::spicedb_permissions_check_permission)
            }
            .unwrap();
            let _ = CheckPermissionResponse::decode(&resp[..]).unwrap();
        }
        let elapsed = start.elapsed();

        let num_checks_u32 = u32::try_from(NUM_CHECKS).unwrap();
        println!("Total time for {NUM_CHECKS} checks: {elapsed:?}");
        println!("Average per check: {:?}", elapsed / num_checks_u32);
        println!(
            "Checks per second: {:.0}",
            f64::from(num_checks_u32) / elapsed.as_secs_f64()
        );

        let result = unsafe { native::spicedb_dispose(handle) };
        if !result.is_null() {
            unsafe { native::spicedb_free(result) };
        }
    }

    /// Performance test (memory transport): Add individual relationships.
    #[tokio::test]
    #[ignore = "performance test - run manually with --ignored flag"]
    async fn perf_memory_add_individual_relationships() {
        const NUM_ADDS: usize = 50;
        use std::time::Instant;

        let (handle, _guard) = start_memory_server().unwrap();
        let mut schema_bytes = Vec::new();
        WriteSchemaRequest {
            schema: TEST_SCHEMA.to_string(),
        }
        .encode(&mut schema_bytes)
        .unwrap();
        unsafe {
            call_memory_rpc(handle, &schema_bytes, native::spicedb_schema_write_schema)
        }
        .unwrap();

        println!("\n=== Performance (memory transport): Add individual relationships ===");

        let start = Instant::now();
        for i in 0..NUM_ADDS {
            let req = WriteRelationshipsRequest {
                updates: vec![RelationshipUpdate {
                    operation: Operation::Touch as i32,
                    relationship: Some(rel(
                        &format!("document:perf{i}"),
                        "reader",
                        "user:alice",
                    )),
                }],
                optional_preconditions: vec![],
            };
            let mut buf = Vec::new();
            req.encode(&mut buf).unwrap();
            unsafe {
                call_memory_rpc(handle, &buf, native::spicedb_permissions_write_relationships)
            }
            .unwrap();
        }
        let elapsed = start.elapsed();

        let num_adds_u32 = u32::try_from(NUM_ADDS).unwrap();
        println!("Total time for {NUM_ADDS} individual adds: {elapsed:?}");
        println!("Average per add: {:?}", elapsed / num_adds_u32);
        println!(
            "Adds per second: {:.0}",
            f64::from(num_adds_u32) / elapsed.as_secs_f64()
        );

        let result = unsafe { native::spicedb_dispose(handle) };
        if !result.is_null() {
            unsafe { native::spicedb_free(result) };
        }
    }

    /// Performance test (memory transport): Bulk write relationships.
    #[tokio::test]
    #[ignore = "performance test - run manually with --ignored flag"]
    async fn perf_memory_bulk_write_relationships() {
        use std::time::Instant;

        let (handle, _guard) = start_memory_server().unwrap();
        let mut schema_bytes = Vec::new();
        WriteSchemaRequest {
            schema: TEST_SCHEMA.to_string(),
        }
        .encode(&mut schema_bytes)
        .unwrap();
        unsafe {
            call_memory_rpc(handle, &schema_bytes, native::spicedb_schema_write_schema)
        }
        .unwrap();

        println!("\n=== Performance (memory transport): Bulk write relationships ===");

        for batch_size in [5_i32, 10, 20, 50] {
            let batch_size_u32 = u32::try_from(batch_size).unwrap();
            let relationships: Vec<Relationship> = (0..batch_size)
                .map(|i| {
                    rel(
                        &format!("document:bulk{batch_size}_{i}"),
                        "reader",
                        "user:alice",
                    )
                })
                .collect();
            let updates: Vec<RelationshipUpdate> = relationships
                .iter()
                .map(|r| RelationshipUpdate {
                    operation: Operation::Touch as i32,
                    relationship: Some(r.clone()),
                })
                .collect();
            let mut buf = Vec::new();
            WriteRelationshipsRequest {
                updates,
                optional_preconditions: vec![],
            }
            .encode(&mut buf)
            .unwrap();

            let start = Instant::now();
            unsafe {
                call_memory_rpc(handle, &buf, native::spicedb_permissions_write_relationships)
            }
            .unwrap();
            let elapsed = start.elapsed();

            println!(
                "Batch of {} relationships: {:?} ({:?} per relationship)",
                batch_size,
                elapsed,
                elapsed / batch_size_u32
            );
        }

        println!("\n--- Comparison: 10 individual vs 10 bulk ---");

        let start = Instant::now();
        for i in 0..10 {
            let req = WriteRelationshipsRequest {
                updates: vec![RelationshipUpdate {
                    operation: Operation::Touch as i32,
                    relationship: Some(rel(
                        &format!("document:cmp_ind{i}"),
                        "reader",
                        "user:bob",
                    )),
                }],
                optional_preconditions: vec![],
            };
            let mut buf = Vec::new();
            req.encode(&mut buf).unwrap();
            unsafe {
                call_memory_rpc(handle, &buf, native::spicedb_permissions_write_relationships)
            }
            .unwrap();
        }
        let individual_time = start.elapsed();
        println!("10 individual adds: {individual_time:?}");

        let relationships: Vec<Relationship> = (0..10)
            .map(|i| rel(&format!("document:cmp_bulk{i}"), "reader", "user:bob"))
            .collect();
        let updates: Vec<RelationshipUpdate> = relationships
            .iter()
            .map(|r| RelationshipUpdate {
                operation: Operation::Touch as i32,
                relationship: Some(r.clone()),
            })
            .collect();
        let mut buf = Vec::new();
        WriteRelationshipsRequest {
            updates,
            optional_preconditions: vec![],
        }
        .encode(&mut buf)
        .unwrap();
        let start = Instant::now();
        unsafe {
            call_memory_rpc(handle, &buf, native::spicedb_permissions_write_relationships)
        }
        .unwrap();
        let bulk_time = start.elapsed();
        println!("10 bulk add: {bulk_time:?}");
        println!(
            "Speedup: {:.1}x",
            individual_time.as_secs_f64() / bulk_time.as_secs_f64()
        );

        let result = unsafe { native::spicedb_dispose(handle) };
        if !result.is_null() {
            unsafe { native::spicedb_free(result) };
        }
    }

    /// Performance test (memory transport): 50,000 relationships.
    #[tokio::test]
    #[ignore = "performance test - run manually with --ignored flag"]
    async fn perf_memory_embedded_50k_relationships() {
        const TOTAL_RELS: usize = 50_000;
        const BATCH_SIZE: usize = 1000;
        const NUM_CHECKS: usize = 500;
        use std::time::Instant;

        let (handle, _guard) = start_memory_server().unwrap();
        let mut schema_bytes = Vec::new();
        WriteSchemaRequest {
            schema: TEST_SCHEMA.to_string(),
        }
        .encode(&mut schema_bytes)
        .unwrap();
        unsafe {
            call_memory_rpc(handle, &schema_bytes, native::spicedb_schema_write_schema)
        }
        .unwrap();

        println!("\n=== Performance (memory transport): 50,000 relationships ===");

        println!("Creating instance (memory) done.");
        println!("Adding {TOTAL_RELS} relationships in batches of {BATCH_SIZE}...");
        let start = Instant::now();
        for batch_num in 0..(TOTAL_RELS / BATCH_SIZE) {
            let batch_start = batch_num * BATCH_SIZE;
            let relationships: Vec<Relationship> = (batch_start..batch_start + BATCH_SIZE)
                .map(|i| {
                    rel(
                        &format!("document:doc{i}"),
                        "reader",
                        &format!("user:user{}", i % 1000),
                    )
                })
                .collect();
            let updates: Vec<RelationshipUpdate> = relationships
                .iter()
                .map(|r| RelationshipUpdate {
                    operation: Operation::Touch as i32,
                    relationship: Some(r.clone()),
                })
                .collect();
            let mut buf = Vec::new();
            WriteRelationshipsRequest {
                updates,
                optional_preconditions: vec![],
            }
            .encode(&mut buf)
            .unwrap();
            unsafe {
                call_memory_rpc(handle, &buf, native::spicedb_permissions_write_relationships)
            }
            .unwrap();
        }
        println!(
            "Total time to add {} relationships: {:?}",
            TOTAL_RELS,
            start.elapsed()
        );

        let warm = check_req("document:doc0", "read", "user:user0");
        let mut warm_bytes = Vec::new();
        warm.encode(&mut warm_bytes).unwrap();
        for _ in 0..20 {
            let _ = unsafe {
                call_memory_rpc(handle, &warm_bytes, native::spicedb_permissions_check_permission)
            };
        }

        let num_checks_u32 = u32::try_from(NUM_CHECKS).unwrap();
        let start = Instant::now();
        for i in 0..NUM_CHECKS {
            let req = check_req(
                &format!("document:doc{}", i % TOTAL_RELS),
                "read",
                &format!("user:user{}", i % 1000),
            );
            let mut buf = Vec::new();
            req.encode(&mut buf).unwrap();
            let resp = unsafe {
                call_memory_rpc(handle, &buf, native::spicedb_permissions_check_permission)
            }
            .unwrap();
            let _ = CheckPermissionResponse::decode(&resp[..]).unwrap();
        }
        let elapsed = start.elapsed();

        println!("Total time for {NUM_CHECKS} checks: {elapsed:?}");
        println!("Average per check: {:?}", elapsed / num_checks_u32);
        println!(
            "Checks per second: {:.0}",
            f64::from(num_checks_u32) / elapsed.as_secs_f64()
        );

        let start = Instant::now();
        for i in 0..NUM_CHECKS {
            let req = check_req(
                &format!("document:doc{}", i % TOTAL_RELS),
                "read",
                "user:nonexistent",
            );
            let mut buf = Vec::new();
            req.encode(&mut buf).unwrap();
            let _ = unsafe {
                call_memory_rpc(handle, &buf, native::spicedb_permissions_check_permission)
            };
        }
        let elapsed = start.elapsed();
        println!("\nNegative checks (user not found):");
        println!("Average per check: {:?}", elapsed / num_checks_u32);

        let result = unsafe { native::spicedb_dispose(handle) };
        if !result.is_null() {
            unsafe { native::spicedb_free(result) };
        }
    }

    /// Shared-datastore tests: two embedded servers using the same remote datastore.
    /// Run with: cargo test --ignored `datastore_shared`
    ///
    /// Requires: Docker (linux/amd64 images), and `spicedb` CLI in PATH for migrations.
    /// Only run on `x86_64`: amd64 containers fail on arm64 (exec format error / QEMU).
    #[cfg(all(not(target_os = "windows"), target_arch = "x86_64"))]
    mod datastore_shared {
        /// Platform for testcontainers: use Linux so images work on Windows Docker Desktop.
        const LINUX_AMD64: &str = "linux/amd64";
        use std::process::Command;

        use testcontainers_modules::{
            cockroach_db, mysql, postgres,
            testcontainers::{
                GenericImage, ImageExt,
                core::{IntoContainerPort, WaitFor},
                runners::AsyncRunner,
            },
        };

        use super::*;
        use crate::StartOptions;

        /// Run `spicedb datastore migrate head` for the given engine and URI.
        /// Returns Ok(()) on success, Err on failure. Fails the test if spicedb not found.
        fn run_migrate(engine: &str, uri: &str, extra_args: &[(&str, &str)]) -> Result<(), String> {
            let mut cmd = Command::new("spicedb");
            cmd.args([
                "datastore",
                "migrate",
                "head",
                "--datastore-engine",
                engine,
                "--datastore-conn-uri",
                uri,
            ]);
            for (k, v) in extra_args {
                cmd.arg(format!("--{k}={v}"));
            }
            let output = cmd
                .output()
                .map_err(|e| format!("spicedb migrate failed (is spicedb in PATH?): {e}"))?;
            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("spicedb migrate failed: {stderr}"))
            }
        }

        /// Two servers, shared datastore: write via server 1, read via server 2.
        async fn run_shared_datastore_test(datastore: &str, datastore_uri: &str) {
            run_migrate(datastore, datastore_uri, &[]).expect("migration must succeed");

            let opts = StartOptions {
                datastore: Some(datastore.into()),
                datastore_uri: Some(datastore_uri.into()),
                grpc_transport: Some("tcp".into()),
                ..Default::default()
            };

            let schema = TEST_SCHEMA;
            let db1 = EmbeddedSpiceDB::new_with_options(schema, &[], Some(&opts))
                .await
                .unwrap();
            let db2 = EmbeddedSpiceDB::new_with_options(schema, &[], Some(&opts))
                .await
                .unwrap();

            // Write via server 1
            db1.permissions()
                .write_relationships(tonic::Request::new(WriteRelationshipsRequest {
                    updates: vec![RelationshipUpdate {
                        operation: Operation::Touch as i32,
                        relationship: Some(rel("document:shared", "reader", "user:alice")),
                    }],
                    optional_preconditions: vec![],
                }))
                .await
                .unwrap();

            // Read via server 2 (shared datastore)
            let r = db2
                .permissions()
                .check_permission(tonic::Request::new(check_req(
                    "document:shared",
                    "read",
                    "user:alice",
                )))
                .await
                .unwrap();
            assert_eq!(
                r.into_inner().permissionship,
                Permissionship::HasPermission as i32
            );
        }

        #[tokio::test]
        async fn datastore_shared_postgres() {
            // PostgreSQL 17+ required for xid8 type (SpiceDB add-xid-columns migration)
            let container = postgres::Postgres::default()
                .with_tag("17")
                .with_platform(LINUX_AMD64)
                .start()
                .await
                .unwrap();
            let host = container.get_host().await.unwrap();
            let port = container.get_host_port_ipv4(5432).await.unwrap();
            let uri = format!("postgres://postgres:postgres@{host}:{port}/postgres");
            run_shared_datastore_test("postgres", &uri).await;
        }

        #[tokio::test]
        async fn datastore_shared_cockroachdb() {
            let container = cockroach_db::CockroachDb::default()
                .with_platform(LINUX_AMD64)
                .start()
                .await
                .unwrap();
            let host = container.get_host().await.unwrap();
            let port = container.get_host_port_ipv4(26257).await.unwrap();
            let uri = format!("postgres://root@{host}:{port}/defaultdb?sslmode=disable");
            run_shared_datastore_test("cockroachdb", &uri).await;
        }

        #[tokio::test]
        async fn datastore_shared_mysql() {
            let container = mysql::Mysql::default()
                .with_platform(LINUX_AMD64)
                .start()
                .await
                .unwrap();
            let host = container.get_host().await.unwrap();
            let port = container.get_host_port_ipv4(3306).await.unwrap();
            // MySQL: user@tcp(host:port)/db format; parseTime=true required by SpiceDB
            let uri = format!("root@tcp({host}:{port})/test?parseTime=true");
            run_shared_datastore_test("mysql", &uri).await;
        }

        #[tokio::test]
        async fn datastore_shared_spanner() {
            // roryq/spanner-emulator: creates instance + database on startup via env vars (no gcloud exec)
            // Call GenericImage methods (with_exposed_port, with_wait_for) before ImageExt methods (with_platform, with_env_var)
            let container = GenericImage::new("roryq/spanner-emulator", "latest")
                .with_exposed_port(9010u16.tcp())
                .with_wait_for(WaitFor::seconds(5))
                .with_platform(LINUX_AMD64)
                .with_env_var("SPANNER_PROJECT_ID", "test-project")
                .with_env_var("SPANNER_INSTANCE_ID", "test-instance")
                .with_env_var("SPANNER_DATABASE_ID", "test-db")
                .start()
                .await
                .unwrap();
            let host = container.get_host().await.unwrap();
            let port = container.get_host_port_ipv4(9010u16.tcp()).await.unwrap();
            let emulator_host = format!("{host}:{port}");
            let uri = "projects/test-project/instances/test-instance/databases/test-db".to_string();

            run_migrate(
                "spanner",
                &uri,
                &[("datastore-spanner-emulator-host", &emulator_host)],
            )
            .expect("migration must succeed");

            let opts = StartOptions {
                datastore: Some("spanner".into()),
                datastore_uri: Some(uri),
                grpc_transport: Some("tcp".into()),
                spanner_emulator_host: Some(emulator_host),
                ..Default::default()
            };

            let schema = TEST_SCHEMA;
            let db1 = EmbeddedSpiceDB::new_with_options(schema, &[], Some(&opts))
                .await
                .unwrap();
            let db2 = EmbeddedSpiceDB::new_with_options(schema, &[], Some(&opts))
                .await
                .unwrap();

            db1.permissions()
                .write_relationships(tonic::Request::new(WriteRelationshipsRequest {
                    updates: vec![RelationshipUpdate {
                        operation: Operation::Touch as i32,
                        relationship: Some(rel("document:shared", "reader", "user:alice")),
                    }],
                    optional_preconditions: vec![],
                }))
                .await
                .unwrap();

            let r = db2
                .permissions()
                .check_permission(tonic::Request::new(check_req(
                    "document:shared",
                    "read",
                    "user:alice",
                )))
                .await
                .unwrap();
            assert_eq!(
                r.into_inner().permissionship,
                Permissionship::HasPermission as i32
            );
        }
    }
}
