# spicedb-embedded

Embedded [SpiceDB](https://authzed.com/spicedb) for use in application tests and development. This repository provides an in-memory SpiceDB server that you can communicate with over gRPC using Unix sockets.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  Your Application (Rust, Python, etc.)                          │
├─────────────────────────────────────────────────────────────────┤
│  Language bindings (rust/, python/, ...)                         │
│  - FFI/cbindgen to shared/c                                     │
│  - Native gRPC (tonic/protobuf) over Unix socket                 │
├─────────────────────────────────────────────────────────────────┤
│  shared/c: C-shared library (Go/CGO)                             │
│  - Embeds SpiceDB server                                        │
│  - In-memory datastore, one instance per handle                  │
│  - Listens on unique Unix socket per instance                   │
└─────────────────────────────────────────────────────────────────┘
```

The **shared/c** library is the foundation—all language bindings build on top of it via C FFI. Each instance is independent, enabling parallel testing.

## Quick Start (Rust)

`EmbeddedSpiceDB` is a thin wrapper that connects auto-generated tonic gRPC clients over a Unix socket. Use the full SpiceDB API via [`permissions()`](https://docs.rs/spicedb-grpc/latest/spicedb_grpc/authzed/api/v1/permissions_service_client/struct.PermissionsServiceClient.html), [`schema()`](https://docs.rs/spicedb-grpc/latest/spicedb_grpc/authzed/api/v1/schema_service_client/struct.SchemaServiceClient.html), and [`watch()`](https://docs.rs/spicedb-grpc/latest/spicedb_grpc/authzed/api/v1/watch_service_client/struct.WatchServiceClient.html):

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
    resource: Some(v1::ObjectReference { object_type: "document".into(), object_id: "readme".into() }),
    relation: "reader".into(),
    subject: Some(v1::SubjectReference {
        object: Some(v1::ObjectReference { object_type: "user".into(), object_id: "alice".into() }),
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
        resource: Some(v1::ObjectReference { object_type: "document".into(), object_id: "readme".into() }),
        permission: "read".into(),
        subject: Some(v1::SubjectReference {
            object: Some(v1::ObjectReference { object_type: "user".into(), object_id: "alice".into() }),
            optional_relation: String::new(),
        }),
        context: None,
        with_tracing: false,
    }))
    .await?;
let allowed = response.into_inner().permissionship == v1::check_permission_response::Permissionship::HasPermission as i32;
assert!(allowed);
```

## Prerequisites

- **Go** 1.23+ (for building shared/c)
- **Rust** (for the Rust crate)
- **CGO** enabled (for Go build)

## Building

### 1. Build the shared/c library

```bash
mise run shared-c-build
# or manually:
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.dylib .  # macOS
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.so .      # Linux
```

### 2. Build and test the Rust crate

The Rust build script will build shared/c automatically if needed. To build and test:

```bash
cd rust && cargo build
cargo test
```

Or from the repo root:

```bash
mise run check   # runs clippy, fmt, test, cargo-deny
```

## Directory Structure

| Directory | Description |
|-----------|-------------|
| `shared/c/` | Go/CGO library that embeds SpiceDB. Exposes `spicedb_new`, `spicedb_dispose`, `spicedb_free` via C FFI. |
| `rust/`    | Rust crate `spicedb-embedded` — thin FFI wrapper + [spicedb-grpc](https://docs.rs/spicedb-grpc) clients. |

Future: `python/`, `go/`, etc. for other languages with C FFI support.

## License

Apache-2.0
