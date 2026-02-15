# spicedb-grpc-tonic

Authzed/SpiceDB gRPC API types and client stubs generated from [buf.build/authzed/api](https://buf.build/authzed/api) using [tonic](https://github.com/hyperium/tonic).

## What this crate provides

- **Protobuf types** for the Authzed API (e.g. `CheckPermissionRequest`, `Relationship`, `Consistency`)
- **Tonic client stubs** for the Permissions and Schema services (e.g. `PermissionsServiceClient`)

Protos are exported with `buf export` at build time and compiled with `tonic-build`; only the client side is built (no server code).

## Usage

Add to `Cargo.toml`:

```toml
[dependencies]
spicedb-grpc-tonic = "0.1"
```

Use the re-exported Authzed API v1 types and clients:

```rust
use spicedb_grpc_tonic::v1::{
    CheckPermissionRequest,
    PermissionsServiceClient,
    Relationship,
};
```

## Build requirements

The build script runs `buf` to export protos from the Buf Schema Registry. You need:

- [buf](https://buf.build/docs/installation)
- [protoc](https://grpc.io/docs/protoc-installation/) (used by `tonic-build`)

## License

Apache-2.0
