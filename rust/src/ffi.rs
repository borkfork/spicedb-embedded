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
// Platform-specific FFI is provided by the corresponding -sys crate.
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
use spicedb_embedded_sys_linux_arm64 as native;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use spicedb_embedded_sys_linux_x64 as native;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use spicedb_embedded_sys_osx_arm64 as native;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
use spicedb_embedded_sys_win_x64 as native;
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
    use spicedb_grpc::authzed::api::v1::{
        CheckPermissionRequest, Consistency, ObjectReference, ReadRelationshipsRequest,
        Relationship, RelationshipFilter, RelationshipUpdate, SubjectReference,
        WriteRelationshipsRequest, relationship_update::Operation,
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

    /// Shared-datastore tests: two embedded servers using the same remote datastore.
    /// Run with: cargo test --ignored `datastore_shared`
    ///
    /// Requires: Docker, and `spicedb` CLI in PATH for migrations.
    /// Disabled on Windows (does not work in Github Actions).
    #[cfg(not(target_os = "windows"))]
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
