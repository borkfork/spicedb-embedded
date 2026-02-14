//! Embedded `SpiceDB` using CGO FFI with native gRPC.
//!
//! This crate provides an in-process `SpiceDB` instance for authorization checks.
//! It uses a C-shared library to start `SpiceDB` servers, then connects via Unix
//! socket. All API access is through tonic clients generated from
//! [buf.build/authzed/api](https://buf.build/authzed/api) (see the `spicedb-api` crate).
//!
//! # Example
//!
//! ```ignore
//! use spicedb_embedded::{v1, EmbeddedSpiceDB};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let schema = r#"
//! definition user {}
//! definition document {
//!     relation reader: user
//!     permission read = reader
//! }
//! "#;
//!
//!     let relationships = vec![v1::Relationship {
//!         resource: Some(v1::ObjectReference { object_type: "document".into(), object_id: "readme".into() }),
//!         relation: "reader".into(),
//!         subject: Some(v1::SubjectReference {
//!             object: Some(v1::ObjectReference { object_type: "user".into(), object_id: "alice".into() }),
//!             optional_relation: String::new(),
//!         }),
//!         optional_caveat: None,
//!     }];
//!
//!     let spicedb = EmbeddedSpiceDB::new(schema, &relationships).await?;
//!     let mut permissions = spicedb.permissions();
//!     // Use the full SpiceDB API via the generated client
//!     let response = permissions.check_permission(tonic::Request::new(v1::CheckPermissionRequest {
//!         consistency: None,
//!         resource: Some(v1::ObjectReference { object_type: "document".into(), object_id: "readme".into() }),
//!         permission: "read".into(),
//!         subject: Some(v1::SubjectReference {
//!             object: Some(v1::ObjectReference { object_type: "user".into(), object_id: "alice".into() }),
//!             optional_relation: String::new(),
//!         }),
//!         context: None,
//!         with_tracing: false,
//!     })).await?;
//!     Ok(())
//! }
//! ```

mod spicedb;

pub use spicedb::{
    EmbeddedSpiceDB, MemoryPermissionsClient, MemorySchemaClient, MemorySpiceDB, StartOptions,
};
// Re-export spicedb-grpc so users have direct access to all generated types
pub use spicedb_api::v1;

/// Errors from embedded `SpiceDB` operations
#[derive(Debug, thiserror::Error)]
pub enum SpiceDBError {
    /// Failed to load the module (WASM or shared library)
    #[error("failed to load module: {0}")]
    ModuleLoad(String),

    /// Runtime error during execution
    #[error("runtime error: {0}")]
    Runtime(String),

    /// Protocol error in communication
    #[error("protocol error: {0}")]
    Protocol(String),

    /// Error from `SpiceDB` itself
    #[error("SpiceDB error: {0}")]
    SpiceDB(String),
}
