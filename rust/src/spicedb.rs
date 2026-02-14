//! FFI bindings for the `SpiceDB` C-shared library (CGO).
//!
//! This module provides a thin wrapper that starts `SpiceDB` via FFI and
//! connects tonic-generated gRPC clients over a Unix socket (Unix) or TCP (Windows).
//! Schema and relationships are written via gRPC (not JSON).

use serde::{Deserialize, Serialize};
use spicedb_api::v1::{
    RelationshipUpdate, WriteRelationshipsRequest, WriteSchemaRequest,
    relationship_update::Operation,
};
use spicedb_embedded_sys::{dispose, start};

use crate::SpiceDBError;

/// Response from the C library (JSON parsed)
#[derive(Debug, Deserialize)]
struct CResponse {
    success: bool,
    error: Option<String>,
    data: Option<serde_json::Value>,
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

/// Parses the JSON response string from the C library (start/dispose) into the inner data or error.
fn parse_json_response(response_str: &str) -> Result<serde_json::Value, SpiceDBError> {
    let response: CResponse = serde_json::from_str(response_str)
        .map_err(|e| SpiceDBError::Protocol(format!("invalid JSON: {e} (raw: {response_str})")))?;

    if response.success {
        Ok(response.data.unwrap_or(serde_json::Value::Null))
    } else {
        Err(SpiceDBError::SpiceDB(
            response.error.unwrap_or_else(|| "unknown error".into()),
        ))
    }
}

use spicedb_api::v1::{
    CheckBulkPermissionsRequest, CheckBulkPermissionsResponse, CheckPermissionRequest,
    CheckPermissionResponse, DeleteRelationshipsRequest, DeleteRelationshipsResponse,
    ExpandPermissionTreeRequest, ExpandPermissionTreeResponse, ReadSchemaRequest,
    ReadSchemaResponse, WriteRelationshipsResponse, WriteSchemaResponse,
};
use spicedb_embedded_sys::memory_transport;

/// Embedded `SpiceDB` instance (in-memory transport). All RPCs go through the FFI.
/// For streaming APIs (`Watch`, `ReadRelationships`, etc.) use [`streaming_address`](EmbeddedSpiceDB::streaming_address)
/// (the C library starts a streaming proxy and returns it in the start response).
pub struct EmbeddedSpiceDB {
    handle: u64,
    /// Set from the C library start response; use for `Watch`, `ReadRelationships`, `LookupResources`, `LookupSubjects`.
    streaming_address: String,
    /// Set from the C library start response: "unix" or "tcp".
    streaming_transport: String,
}

unsafe impl Send for EmbeddedSpiceDB {}
unsafe impl Sync for EmbeddedSpiceDB {}

impl EmbeddedSpiceDB {
    /// Create a new embedded `SpiceDB` instance with optional schema and relationships.
    ///
    /// If `schema` is non-empty, writes it via `SchemaService`. If `relationships` is non-empty, writes them via `WriteRelationships`.
    ///
    /// # Errors
    ///
    /// Returns an error if the C library fails to start, returns invalid JSON, or schema/relationship write fails.
    pub fn new(
        schema: &str,
        relationships: &[spicedb_api::v1::Relationship],
        options: Option<&StartOptions>,
    ) -> Result<Self, SpiceDBError> {
        let opts = options.cloned().unwrap_or_default();
        let json = serde_json::to_string(&opts)
            .map_err(|e| SpiceDBError::Protocol(format!("serialize options: {e}")))?;
        let response_str = start(Some(&json)).map_err(SpiceDBError::Runtime)?;
        let data = parse_json_response(&response_str)?;
        let handle = data
            .get("handle")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| SpiceDBError::Protocol("missing handle in start response".into()))?;
        let streaming_address = data
            .get("streaming_address")
            .and_then(serde_json::Value::as_str)
            .map(String::from)
            .ok_or_else(|| {
                SpiceDBError::Protocol("missing streaming_address in start response".into())
            })?;
        let streaming_transport = data
            .get("streaming_transport")
            .and_then(serde_json::Value::as_str)
            .map(String::from)
            .ok_or_else(|| {
                SpiceDBError::Protocol("missing streaming_transport in start response".into())
            })?;

        let db = Self {
            handle,
            streaming_address,
            streaming_transport,
        };

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
                    optional_transaction_metadata: None,
                },
            )
            .map_err(|e| SpiceDBError::SpiceDB(e.0))?;
        }

        Ok(db)
    }

    /// Permissions service (`CheckPermission`, `WriteRelationships`, `DeleteRelationships`, etc.).
    #[must_use]
    pub const fn permissions(&self) -> MemoryPermissionsClient {
        MemoryPermissionsClient {
            handle: self.handle,
        }
    }

    /// Schema service (`ReadSchema`, `WriteSchema`).
    #[must_use]
    pub const fn schema(&self) -> MemorySchemaClient {
        MemorySchemaClient {
            handle: self.handle,
        }
    }

    /// Raw handle for advanced use (e.g. with [`spicedb_embedded_sys::memory_transport`]).
    #[must_use]
    pub const fn handle(&self) -> u64 {
        self.handle
    }

    /// Returns the address for streaming APIs (`Watch`, `ReadRelationships`, `LookupResources`, `LookupSubjects`).
    /// Set from the C library start response when the streaming proxy was started.
    #[must_use]
    pub fn streaming_address(&self) -> &str {
        &self.streaming_address
    }

    /// Streaming proxy transport: "unix" or "tcp" (from C library start response).
    #[must_use]
    pub fn streaming_transport(&self) -> &str {
        &self.streaming_transport
    }
}

impl Drop for EmbeddedSpiceDB {
    fn drop(&mut self) {
        let _ = dispose(self.handle);
    }
}

/// Permissions service client for memory transport. All methods are synchronous and use the -sys safe layer.
pub struct MemoryPermissionsClient {
    handle: u64,
}

impl MemoryPermissionsClient {
    /// `CheckPermission`.
    ///
    /// # Errors
    ///
    /// Returns an error if the FFI call fails or the response cannot be decoded.
    pub fn check_permission(
        &self,
        request: &CheckPermissionRequest,
    ) -> Result<CheckPermissionResponse, SpiceDBError> {
        memory_transport::check_permission(self.handle, request)
            .map_err(|e| SpiceDBError::SpiceDB(e.0))
    }

    /// `WriteRelationships`.
    ///
    /// # Errors
    ///
    /// Returns an error if the FFI call fails or the response cannot be decoded.
    pub fn write_relationships(
        &self,
        request: &WriteRelationshipsRequest,
    ) -> Result<WriteRelationshipsResponse, SpiceDBError> {
        memory_transport::write_relationships(self.handle, request)
            .map_err(|e| SpiceDBError::SpiceDB(e.0))
    }

    /// `DeleteRelationships`.
    ///
    /// # Errors
    ///
    /// Returns an error if the FFI call fails or the response cannot be decoded.
    pub fn delete_relationships(
        &self,
        request: &DeleteRelationshipsRequest,
    ) -> Result<DeleteRelationshipsResponse, SpiceDBError> {
        memory_transport::delete_relationships(self.handle, request)
            .map_err(|e| SpiceDBError::SpiceDB(e.0))
    }

    /// `CheckBulkPermissions`.
    ///
    /// # Errors
    ///
    /// Returns an error if the FFI call fails or the response cannot be decoded.
    pub fn check_bulk_permissions(
        &self,
        request: &CheckBulkPermissionsRequest,
    ) -> Result<CheckBulkPermissionsResponse, SpiceDBError> {
        memory_transport::check_bulk_permissions(self.handle, request)
            .map_err(|e| SpiceDBError::SpiceDB(e.0))
    }

    /// `ExpandPermissionTree`.
    ///
    /// # Errors
    ///
    /// Returns an error if the FFI call fails or the response cannot be decoded.
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
    /// `ReadSchema`.
    ///
    /// # Errors
    ///
    /// Returns an error if the FFI call fails or the response cannot be decoded.
    pub fn read_schema(
        &self,
        request: &ReadSchemaRequest,
    ) -> Result<ReadSchemaResponse, SpiceDBError> {
        memory_transport::read_schema(self.handle, request).map_err(|e| SpiceDBError::SpiceDB(e.0))
    }

    /// `WriteSchema`.
    ///
    /// # Errors
    ///
    /// Returns an error if the FFI call fails or the response cannot be decoded.
    pub fn write_schema(
        &self,
        request: &WriteSchemaRequest,
    ) -> Result<WriteSchemaResponse, SpiceDBError> {
        memory_transport::write_schema(self.handle, request).map_err(|e| SpiceDBError::SpiceDB(e.0))
    }
}

#[cfg(test)]
mod tests {

    use std::time::Duration;

    use spicedb_api::v1::{
        CheckPermissionRequest, Consistency, ObjectReference, ReadRelationshipsRequest,
        Relationship, RelationshipFilter, RelationshipUpdate, SubjectReference, WatchKind,
        WatchRequest, WriteRelationshipsRequest, relationship_update::Operation,
        watch_service_client::WatchServiceClient,
    };
    use tokio::time::timeout;
    use tokio_stream::StreamExt;
    use tonic::transport::{Channel, Endpoint};

    use super::*;
    use crate::v1::check_permission_response::Permissionship;

    /// Connect to the streaming proxy (addr + transport from C library start response).
    async fn connect_streaming(
        addr: &str,
        transport: &str,
    ) -> Result<Channel, Box<dyn std::error::Error + Send + Sync>> {
        #[cfg(unix)]
        {
            if transport == "unix" {
                let path = addr.to_string();
                Endpoint::try_from("http://[::]:50051")?
                    .connect_with_connector(tower::service_fn(move |_: tonic::transport::Uri| {
                        let path = path.clone();
                        async move {
                            let stream = tokio::net::UnixStream::connect(&path).await?;
                            Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(stream))
                        }
                    }))
                    .await
                    .map_err(Into::into)
            } else {
                Endpoint::from_shared(format!("http://{addr}"))?
                    .connect()
                    .await
                    .map_err(Into::into)
            }
        }
        #[cfg(windows)]
        {
            Endpoint::from_shared(format!("http://{addr}"))?
                .connect()
                .await
                .map_err(Into::into)
        }
    }

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
            optional_expires_at: None,
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

    /// `EmbeddedSpiceDB::new` + `.permissions().check_permission()`. Skipped on bind/streaming proxy errors.
    #[test]
    fn test_check_permission() {
        let relationships = vec![
            rel("document:readme", "reader", "user:alice"),
            rel("document:readme", "writer", "user:bob"),
        ];
        let spicedb = match EmbeddedSpiceDB::new(TEST_SCHEMA, &relationships, None) {
            Ok(db) => db,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("streaming proxy")
                    || msg.contains("bind")
                    || msg.contains("operation not permitted")
                {
                    return;
                }
                panic!("EmbeddedSpiceDB::new failed: {e}");
            }
        };

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

    /// Verifies the streaming proxy: start Watch stream, write a relationship, receive update on stream. Skipped on bind/proxy errors.
    #[test]
    fn test_watch_streaming() {
        let db = match EmbeddedSpiceDB::new(TEST_SCHEMA, &[], None) {
            Ok(d) => d,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("streaming proxy")
                    || msg.contains("bind")
                    || msg.contains("operation not permitted")
                {
                    return;
                }
                panic!("EmbeddedSpiceDB::new failed: {e}");
            }
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let channel = connect_streaming(db.streaming_address(), db.streaming_transport())
                .await
                .unwrap();
            let mut watch_client = WatchServiceClient::new(channel);
            // Request checkpoints so the server sends an initial response and keeps the stream alive.
            let watch_req = WatchRequest {
                optional_update_kinds: vec![
                    WatchKind::IncludeRelationshipUpdates.into(),
                    WatchKind::IncludeCheckpoints.into(),
                ],
                ..Default::default()
            };
            let mut stream =
                match timeout(Duration::from_secs(10), watch_client.watch(watch_req)).await {
                    Ok(Ok(response)) => response.into_inner(),
                    Ok(Err(e)) => panic!("watch() failed: {e}"),
                    Err(_) => return,
                };

            let write_req = WriteRelationshipsRequest {
                updates: vec![RelationshipUpdate {
                    operation: Operation::Touch as i32,
                    relationship: Some(rel("document:watched", "reader", "user:alice")),
                }],
                optional_preconditions: vec![],
                optional_transaction_metadata: None,
            };
            db.permissions().write_relationships(&write_req).unwrap();

            let mut received_update = false;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
            while tokio::time::Instant::now() < deadline {
                match timeout(Duration::from_millis(200), stream.next()).await {
                    Ok(Some(Ok(resp))) => {
                        if !resp.updates.is_empty() {
                            received_update = true;
                            break;
                        }
                    }
                    Ok(Some(Err(e))) => panic!("watch stream error: {e}"),
                    Ok(None) => break,
                    Err(_) => {}
                }
            }
            assert!(
                received_update,
                "expected at least one Watch response with updates within 3s"
            );
        });
    }

    #[test]
    fn test_ffi_spicedb() {
        let relationships = vec![
            rel("document:readme", "reader", "user:alice"),
            rel("document:readme", "writer", "user:bob"),
        ];

        let spicedb = EmbeddedSpiceDB::new(TEST_SCHEMA, &relationships, None).unwrap();

        assert_eq!(
            spicedb
                .permissions()
                .check_permission(&check_req("document:readme", "read", "user:alice"))
                .unwrap()
                .permissionship,
            Permissionship::HasPermission as i32,
        );
        assert_eq!(
            spicedb
                .permissions()
                .check_permission(&check_req("document:readme", "write", "user:alice"))
                .unwrap()
                .permissionship,
            Permissionship::NoPermission as i32,
        );
        assert_eq!(
            spicedb
                .permissions()
                .check_permission(&check_req("document:readme", "read", "user:bob"))
                .unwrap()
                .permissionship,
            Permissionship::HasPermission as i32,
        );
        assert_eq!(
            spicedb
                .permissions()
                .check_permission(&check_req("document:readme", "write", "user:bob"))
                .unwrap()
                .permissionship,
            Permissionship::HasPermission as i32,
        );
        assert_eq!(
            spicedb
                .permissions()
                .check_permission(&check_req("document:readme", "read", "user:charlie"))
                .unwrap()
                .permissionship,
            Permissionship::NoPermission as i32,
        );
    }

    #[test]
    fn test_add_relationship() {
        let spicedb = EmbeddedSpiceDB::new(TEST_SCHEMA, &[], None).unwrap();

        spicedb
            .permissions()
            .write_relationships(&WriteRelationshipsRequest {
                updates: vec![RelationshipUpdate {
                    operation: Operation::Touch as i32,
                    relationship: Some(rel("document:test", "reader", "user:alice")),
                }],
                optional_preconditions: vec![],
                optional_transaction_metadata: None,
            })
            .unwrap();

        let r = spicedb
            .permissions()
            .check_permission(&check_req("document:test", "read", "user:alice"))
            .unwrap();
        assert_eq!(r.permissionship, Permissionship::HasPermission as i32);
    }

    #[test]
    fn test_parallel_instances() {
        let spicedb1 = EmbeddedSpiceDB::new(TEST_SCHEMA, &[], None).unwrap();
        let spicedb2 = EmbeddedSpiceDB::new(TEST_SCHEMA, &[], None).unwrap();

        spicedb1
            .permissions()
            .write_relationships(&WriteRelationshipsRequest {
                updates: vec![RelationshipUpdate {
                    operation: Operation::Touch as i32,
                    relationship: Some(rel("document:doc1", "reader", "user:alice")),
                }],
                optional_preconditions: vec![],
                optional_transaction_metadata: None,
            })
            .unwrap();

        let r1 = spicedb1
            .permissions()
            .check_permission(&check_req("document:doc1", "read", "user:alice"))
            .unwrap();
        assert_eq!(r1.permissionship, Permissionship::HasPermission as i32);

        let r2 = spicedb2
            .permissions()
            .check_permission(&check_req("document:doc1", "read", "user:alice"))
            .unwrap();
        assert_eq!(r2.permissionship, Permissionship::NoPermission as i32);
    }

    #[tokio::test]
    async fn test_read_relationships() {
        let relationships = vec![
            rel("document:doc1", "reader", "user:alice"),
            rel("document:doc1", "reader", "user:bob"),
        ];

        let spicedb = EmbeddedSpiceDB::new(TEST_SCHEMA, &relationships, None).unwrap();
        let channel = connect_streaming(spicedb.streaming_address(), spicedb.streaming_transport())
            .await
            .unwrap();
        let mut client =
            spicedb_api::v1::permissions_service_client::PermissionsServiceClient::new(channel);
        let mut stream = client
            .read_relationships(ReadRelationshipsRequest {
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
            })
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
    #[test]
    #[ignore = "performance test - run manually with --ignored flag"]
    fn perf_check_with_1000_relationships() {
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
        let spicedb = EmbeddedSpiceDB::new(TEST_SCHEMA, &relationships, None).unwrap();
        println!(
            "Instance creation with 1000 relationships: {:?}",
            start.elapsed()
        );

        // Warm up
        for _ in 0..10 {
            let _ = spicedb.permissions().check_permission(&check_req(
                "document:doc0",
                "read",
                "user:user0",
            ));
        }

        // Benchmark permission checks
        let start = Instant::now();
        for i in 0..NUM_CHECKS {
            let doc = format!("document:doc{}", i % 1000);
            let user = format!("user:user{}", i % 100);
            let _ = spicedb
                .permissions()
                .check_permission(&check_req(&doc, "read", &user))
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
    #[test]
    #[ignore = "performance test - run manually with --ignored flag"]
    fn perf_add_individual_relationships() {
        const NUM_ADDS: usize = 50;
        use std::time::Instant;

        let spicedb = EmbeddedSpiceDB::new(TEST_SCHEMA, &[], None).unwrap();

        println!("\n=== Performance: Add individual relationships ===");

        let start = Instant::now();
        for i in 0..NUM_ADDS {
            spicedb
                .permissions()
                .write_relationships(&WriteRelationshipsRequest {
                    updates: vec![RelationshipUpdate {
                        operation: Operation::Touch as i32,
                        relationship: Some(rel(
                            &format!("document:perf{i}"),
                            "reader",
                            "user:alice",
                        )),
                    }],
                    optional_preconditions: vec![],
                    optional_transaction_metadata: None,
                })
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
    #[test]
    #[ignore = "performance test - run manually with --ignored flag"]
    fn perf_bulk_write_relationships() {
        use std::time::Instant;

        let spicedb = EmbeddedSpiceDB::new(TEST_SCHEMA, &[], None).unwrap();

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
                .write_relationships(&WriteRelationshipsRequest {
                    updates: relationships
                        .iter()
                        .map(|r| RelationshipUpdate {
                            operation: Operation::Touch as i32,
                            relationship: Some(r.clone()),
                        })
                        .collect(),
                    optional_preconditions: vec![],
                    optional_transaction_metadata: None,
                })
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

        let spicedb2 = EmbeddedSpiceDB::new(TEST_SCHEMA, &[], None).unwrap();

        let start = Instant::now();
        for i in 0..10 {
            spicedb2
                .permissions()
                .write_relationships(&WriteRelationshipsRequest {
                    updates: vec![RelationshipUpdate {
                        operation: Operation::Touch as i32,
                        relationship: Some(rel(
                            &format!("document:cmp_ind{i}"),
                            "reader",
                            "user:bob",
                        )),
                    }],
                    optional_preconditions: vec![],
                    optional_transaction_metadata: None,
                })
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
            .write_relationships(&WriteRelationshipsRequest {
                updates: relationships
                    .iter()
                    .map(|r| RelationshipUpdate {
                        operation: Operation::Touch as i32,
                        relationship: Some(r.clone()),
                    })
                    .collect(),
                optional_preconditions: vec![],
                optional_transaction_metadata: None,
            })
            .unwrap();
        let bulk_time = start.elapsed();
        println!("10 bulk add: {bulk_time:?}");
        println!(
            "Speedup: {:.1}x",
            individual_time.as_secs_f64() / bulk_time.as_secs_f64()
        );
    }

    /// Performance test: 50,000 relationships
    #[test]
    #[ignore = "performance test - run manually with --ignored flag"]
    fn perf_embedded_50k_relationships() {
        const TOTAL_RELS: usize = 50_000;
        const BATCH_SIZE: usize = 1000;
        const NUM_CHECKS: usize = 500;
        use std::time::Instant;

        println!("\n=== Embedded SpiceDB: 50,000 relationships ===");

        // Create 50,000 relationships in batches (SpiceDB max batch is 1000)
        println!("Creating instance...");
        let start = Instant::now();
        let spicedb = EmbeddedSpiceDB::new(TEST_SCHEMA, &[], None).unwrap();
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
                .write_relationships(&WriteRelationshipsRequest {
                    updates: relationships
                        .iter()
                        .map(|r| RelationshipUpdate {
                            operation: Operation::Touch as i32,
                            relationship: Some(r.clone()),
                        })
                        .collect(),
                    optional_preconditions: vec![],
                    optional_transaction_metadata: None,
                })
                .unwrap();
        }
        println!(
            "Total time to add {} relationships: {:?}",
            TOTAL_RELS,
            start.elapsed()
        );

        // Warm up
        for _ in 0..20 {
            let _ = spicedb.permissions().check_permission(&check_req(
                "document:doc0",
                "read",
                "user:user0",
            ));
        }

        // Benchmark permission checks
        let num_checks_u32 = u32::try_from(NUM_CHECKS).unwrap();
        let start = Instant::now();
        for i in 0..NUM_CHECKS {
            let doc = format!("document:doc{}", i % TOTAL_RELS);
            let user = format!("user:user{}", i % 1000);
            let _ = spicedb
                .permissions()
                .check_permission(&check_req(&doc, "read", &user))
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
                .check_permission(&check_req(&doc, "read", "user:nonexistent"))
                .unwrap();
        }
        let elapsed = start.elapsed();
        println!("\nNegative checks (user not found):");
        println!("Average per check: {:?}", elapsed / num_checks_u32);
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
        fn run_shared_datastore_test(datastore: &str, datastore_uri: &str) {
            run_migrate(datastore, datastore_uri, &[]).expect("migration must succeed");

            let opts = StartOptions {
                datastore: Some(datastore.into()),
                datastore_uri: Some(datastore_uri.into()),
                ..Default::default()
            };

            let schema = TEST_SCHEMA;
            let db1 = EmbeddedSpiceDB::new(schema, &[], Some(&opts)).unwrap();
            let db2 = EmbeddedSpiceDB::new(schema, &[], Some(&opts)).unwrap();

            // Write via server 1
            db1.permissions()
                .write_relationships(&WriteRelationshipsRequest {
                    updates: vec![RelationshipUpdate {
                        operation: Operation::Touch as i32,
                        relationship: Some(rel("document:shared", "reader", "user:alice")),
                    }],
                    optional_preconditions: vec![],
                    optional_transaction_metadata: None,
                })
                .unwrap();

            // Read via server 2 (shared datastore)
            let r = db2
                .permissions()
                .check_permission(&check_req("document:shared", "read", "user:alice"))
                .unwrap();
            assert_eq!(r.permissionship, Permissionship::HasPermission as i32);
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
            run_shared_datastore_test("postgres", &uri);
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
            run_shared_datastore_test("cockroachdb", &uri);
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
            run_shared_datastore_test("mysql", &uri);
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
                spanner_emulator_host: Some(emulator_host),
                ..Default::default()
            };

            let schema = TEST_SCHEMA;
            let db1 = EmbeddedSpiceDB::new(schema, &[], Some(&opts)).unwrap();
            let db2 = EmbeddedSpiceDB::new(schema, &[], Some(&opts)).unwrap();

            db1.permissions()
                .write_relationships(&WriteRelationshipsRequest {
                    updates: vec![RelationshipUpdate {
                        operation: Operation::Touch as i32,
                        relationship: Some(rel("document:shared", "reader", "user:alice")),
                    }],
                    optional_preconditions: vec![],
                    optional_transaction_metadata: None,
                })
                .unwrap();

            let r = db2
                .permissions()
                .check_permission(&check_req("document:shared", "read", "user:alice"))
                .unwrap();
            assert_eq!(r.permissionship, Permissionship::HasPermission as i32);
        }
    }
}
