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

let spicedb = EmbeddedSpiceDB::new(schema, &relationships, None)?;

let response = spicedb
    .permissions()
    .check_permission(&v1::CheckPermissionRequest {
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
    })?;

let allowed = response.permissionship
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

let spicedb = EmbeddedSpiceDB::new(schema, &[], Some(&options))?;
// Use full Permissions API (write_relationships, check_permission, etc.)
```

## Running code written in Go compiled to a C-shared library within my service sounds scary

It is scary! Using a C-shared library via FFI bindings introduces memory management in languages that don't typically have to worry about it.

That being said, the SpiceDB code still runs in a Go runtime with garbage collection, which is where the vast majority of time is spent. To help mitigate some of the risk, the FFI layer is kept as straightforward as possible. protobuf is marshalled and unmarshalled at the FFI <--> language runtime boundary in a standardized way. After unmarshalling, requests are sent directly to the SpiceDB server, and responses are returned directly back to the language runtime (after marshalling).

The embedded server uses in-memory transport: unary RPCs (CheckPermission, WriteRelationships, etc.) go through the FFI layer; streaming RPCs (Watch, ReadRelationships) use a small proxy and `streaming_address()` to connect over a socket.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
spicedb-embedded = { path = "../spicedb-embedded/rust" }
# Or from crates.io when published:
# spicedb-embedded = "0.1"
```

**Prerequisites:** The main crate `spicedb-embedded` depends on `spicedb-embedded-sys`, which either builds the C shared library from Go (when in this repo with Go installed) or downloads the prebuilt artifact for the current target. For local development, run `mise run shared-c-build` then `./scripts/stage-all-prebuilds.sh` so that `rust/spicedb-embedded-sys/prebuilds/<rid>/` is populated; otherwise the `-sys` crate will build from `shared/c` if Go is available, or download from the GitHub release when using the published crate.

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

let spicedb = EmbeddedSpiceDB::new(schema, &relationships, None)?;

// Check permission (sync API)
let response = spicedb
    .permissions()
    .check_permission(&v1::CheckPermissionRequest {
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
    })?;

let allowed = response.permissionship
    == v1::check_permission_response::Permissionship::HasPermission as i32;
```

## API

- **`EmbeddedSpiceDB::new(schema, relationships, options)`** — Create an instance (sync). Pass `None` for options to use defaults; use `Some(&StartOptions { ... })` for datastore, etc.
- **`permissions()`** — Sync client for CheckPermission, WriteRelationships, DeleteRelationships, etc.
- **`schema()`** — Sync client for ReadSchema, WriteSchema.
- **`streaming_address()`** — Address for streaming RPCs (Watch, ReadRelationships); connect a gRPC client to this address.

All types are re-exported from `spicedb_grpc_tonic::v1` (generated from buf.build/authzed/api) as `spicedb_embedded::v1`.

### StartOptions

```rust
use spicedb_embedded::{v1, EmbeddedSpiceDB, StartOptions};

let options = StartOptions {
    datastore: Some("memory".into()),           // or "postgres", "cockroachdb", "spanner", "mysql"
    datastore_uri: Some("postgres://...".into()), // required for remote
    spanner_credentials_file: None,
    spanner_emulator_host: None,
    mysql_table_prefix: None,
    metrics_enabled: None, // default false; set Some(true) to enable Prometheus metrics
    ..Default::default()
};

let spicedb = EmbeddedSpiceDB::new(schema, &relationships, Some(&options))?;
```

## Building & Testing

```bash
cd rust
cargo build
cargo test
```

Or from the repo root: `mise run check`
