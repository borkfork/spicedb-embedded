//! FFI bindings for the `SpiceDB` C-shared library (CGO).
//!
//! This module provides a thin wrapper that starts `SpiceDB` via FFI and
//! connects tonic-generated gRPC clients over a Unix socket (Unix) or TCP (Windows).
//! Schema and relationships are written via gRPC (not JSON).

use std::{
    ffi::CStr,
    os::raw::{c_char, c_ulonglong},
};

use serde::Deserialize;
use spicedb_grpc::authzed::api::v1::{
    permissions_service_client::PermissionsServiceClient, relationship_update::Operation,
    schema_service_client::SchemaServiceClient, watch_service_client::WatchServiceClient,
    RelationshipUpdate, WriteRelationshipsRequest, WriteSchemaRequest,
};
#[cfg(unix)]
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint};
#[cfg(unix)]
use tonic::transport::Uri;
#[cfg(unix)]
use tower::service_fn;
use tracing::debug;

use crate::SpiceDBError;

// FFI declarations for the C-shared library
#[link(name = "spicedb")]
unsafe extern "C" {
    fn spicedb_start() -> *mut c_char;
    fn spicedb_dispose(handle: c_ulonglong) -> *mut c_char;
    fn spicedb_free(ptr: *mut c_char);
}

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
    transport: String,
    address: String,
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
    unsafe { spicedb_free(ptr) };

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
    /// socket, then bootstraps schema and relationships via gRPC.
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
        debug!(
            "Starting SpiceDB instance with schema ({} bytes), {} relationships",
            schema.len(),
            relationships.len()
        );

        let data = unsafe {
            let result = spicedb_start();
            call_and_parse(result)?
        };

        let new_resp: NewResponse = serde_json::from_value(data)
            .map_err(|e| SpiceDBError::Protocol(format!("invalid new response: {e}")))?;

        debug!(
            "Connecting to SpiceDB at {} ({})",
            new_resp.address, new_resp.transport
        );

        let channel = connect_to_spicedb(&new_resp.transport, &new_resp.address)
            .await
            .map_err(|e| {
                unsafe {
                    let result = spicedb_dispose(new_resp.handle);
                    if !result.is_null() {
                        spicedb_free(result);
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
            let result = spicedb_dispose(self.handle);
            if !result.is_null() {
                spicedb_free(result);
            }
        }
    }
}

#[cfg(unix)]
async fn connect_to_spicedb(
    _transport: &str,
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
async fn connect_to_spicedb(
    _transport: &str,
    address: &str,
) -> Result<Channel, Box<dyn std::error::Error + Send + Sync>> {
    let channel = Endpoint::from_shared(format!("http://{}", address))?
        .connect()
        .await?;
    Ok(channel)
}

#[cfg(test)]
mod tests {
    use spicedb_grpc::authzed::api::v1::{
        relationship_update::Operation, CheckPermissionRequest, Consistency, ObjectReference,
        ReadRelationshipsRequest, Relationship, RelationshipFilter, RelationshipUpdate,
        SubjectReference, WriteRelationshipsRequest,
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
}
