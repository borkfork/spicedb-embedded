# spicedb-embedded (Rust)

Embedded [SpiceDB](https://authzed.com/spicedb) for Rust — authorization server for tests and development. Uses the shared/c C library via FFI and connects over gRPC via Unix sockets or TCP. Supports memory (default), postgres, cockroachdb, spanner, and mysql datastores.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
spicedb-embedded = { path = "../spicedb-embedded/rust" }
# Or from crates.io when published:
# spicedb-embedded = "0.1"
```

**Prerequisites:** Go 1.23+ with CGO enabled (to build the shared library). The Rust build script compiles shared/c automatically.

## Usage

```rust
use spicedb_embedded::{v1, EmbeddedSpiceDB};

let schema = r#"
definition user {}
definition document {
    relation reader: user
    permission read = reader
}
"#;

let relationships = vec![v1::Relationship {
    resource: Some(v1::ObjectReference {
        object_type: "document".into(),
        object_id: "readme".into(),
    }),
    relation: "reader".into(),
    subject: Some(v1::SubjectReference {
        object: Some(v1::ObjectReference {
            object_type: "user".into(),
            object_id: "alice".into(),
        }),
        optional_relation: String::new(),
    }),
    optional_caveat: None,
}];

let spicedb = EmbeddedSpiceDB::new(schema, &relationships).await?;

// Check permission
let response = spicedb
    .permissions()
    .check_permission(tonic::Request::new(v1::CheckPermissionRequest {
        consistency: Some(v1::Consistency {
            requirement: Some(v1::consistency::Requirement::FullyConsistent(true)),
        }),
        resource: Some(v1::ObjectReference {
            object_type: "document".into(),
            object_id: "readme".into(),
        }),
        permission: "read".into(),
        subject: Some(v1::SubjectReference {
            object: Some(v1::ObjectReference {
                object_type: "user".into(),
                object_id: "alice".into(),
            }),
            optional_relation: String::new(),
        }),
        context: None,
        with_tracing: false,
    }))
    .await?;

let allowed = response.into_inner().permissionship
    == v1::check_permission_response::Permissionship::HasPermission as i32;
```

## API

- **`EmbeddedSpiceDB::new(schema, relationships)`** — Create an instance with schema and optional initial relationships.
- **`EmbeddedSpiceDB::new_with_options(schema, relationships, options)`** — Create with `StartOptions` (datastore, `grpc_transport`, etc.). Pass `None` for defaults.
- **`permissions()`** — `PermissionsServiceClient` for CheckPermission, WriteRelationships, ReadRelationships, etc.
- **`schema()`** — `SchemaServiceClient` for ReadSchema, WriteSchema, ReflectSchema, etc.
- **`watch()`** — `WatchServiceClient` for watching relationship changes.

All types are re-exported from `spicedb_grpc::authzed::api::v1` as `spicedb_embedded::v1`.

### StartOptions

```rust
use spicedb_embedded::{v1, EmbeddedSpiceDB, StartOptions};

let options = StartOptions {
    datastore: Some("memory".into()),           // or "postgres", "cockroachdb", "spanner", "mysql"
    grpc_transport: Some("unix".into()),        // or "tcp"; default by platform
    datastore_uri: Some("postgres://...".into()), // required for remote
    spanner_credentials_file: None,
    spanner_emulator_host: None,
    mysql_table_prefix: None,
    metrics_enabled: None, // default false; set Some(true) to enable Prometheus metrics
};

let spicedb = EmbeddedSpiceDB::new_with_options(schema, &relationships, Some(&options)).await?;
```

## Building & Testing

```bash
cd rust
cargo build
cargo test
```

Or from the repo root: `mise run check`
