# spicedb-embedded (Rust)

Sometimes you need a simple way to run access checks without spinning up a new service. This library provides an embedded version of [SpiceDB](https://authzed.com/spicedb) in various languages. Each implementation is based on a C-shared library (compiled from the SpiceDB source code) with a very thin FFI binding on top of it. This means that it runs the native SpiceDB code within your already-running process.

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

## Who should consider using this?

If you want to spin up SpiceDB, but don't want the overhead of managing another service, this might be for you.

If you want an embedded server for your unit tests and don't have access to Docker / Testcontainers, this might be for you.

If you have a schema and set of permissions that are static / readonly, this might be for you.

## Who should avoid using this?

If you live in a world of microservices that each need to perform permission checks, you should almost certainly spin up a centralized SpiceDB deployment.

If you want visibility into metrics for SpiceDB, you should avoid this.

## How does storage work?

The default datastore is "memory" (memdb). If you use this datastore, keep in mind that it will reset on each app startup. This is a great option if you can easily provide your schema and relationships at runtime. This way, there are no external network calls to check relationships at runtime.

If you need a longer term storage, you can use any SpiceDB-compatible datastores.

```rust
use spicedb_embedded::{v1, EmbeddedSpiceDB, StartOptions};

// Run migrations first: spicedb datastore migrate head --datastore-engine postgres --datastore-conn-uri "postgres://..."
let schema = r#"
definition user {}
definition document {
    relation reader: user
    permission read = reader
}
"#;

let options = StartOptions {
    datastore: Some("postgres".into()),
    datastore_uri: Some("postgres://user:pass@localhost:5432/spicedb".into()),
    ..Default::default()
};

let spicedb = EmbeddedSpiceDB::new_with_options(schema, &[], Some(&options)).await?;
// Use full Permissions API (write_relationships, check_permission, etc.)
```

## Running code written in Go compiled to a C-shared library within my service sounds scary

It is scary! Using a C-shared library via FFI bindings introduces memory management in languages that don't typically have to worry about it.

However, this library purposely limits the FFI layer. The only thing it is used for is to spawn the SpiceDB server (and to dispose of it when you shut down the embedded server). Once the SpiceDB server is running, it exposes a gRPC interface that listens over Unix Sockets (default on Linux/macOS) or TCP (default on Windows).

So you get the benefits of (1) using the same generated gRPC code to communicate with SpiceDB that would in a non-embedded world, and (2) communication happens out-of-band so that no memory allocations happen in the FFI layer once the embedded server is running.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
spicedb-embedded = { path = "../spicedb-embedded/rust" }
# Or from crates.io when published:
# spicedb-embedded = "0.1"
```

**Prerequisites:** Go 1.23+ with CGO enabled (to build the shared library). The Rust build script compiles the C library from Go source at build time; the crate does not ship prebuilt `.so`/`.dll`/`.dylib` binaries, so it stays portable across platforms.

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
