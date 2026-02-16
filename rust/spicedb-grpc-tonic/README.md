# spicedb-grpc-tonic

Authzed/SpiceDB gRPC API types and client stubs generated from [buf.build/authzed/api](https://buf.build/authzed/api) using [tonic](https://github.com/hyperium/tonic).

## What this crate provides

- **Protobuf types** for the Authzed API (e.g. `CheckPermissionRequest`, `Relationship`, `Consistency`)
- **Tonic client stubs** for the Permissions and Schema services (e.g. `PermissionsServiceClient`)

The **published crate** ships with generated code in `src/generated/`, so **you do not need buf or protoc** to build. To regenerate the code (e.g. after updating protos), delete `src/generated/` and run the build with [buf](https://buf.build/docs/installation) installed, or run `scripts/regenerate-spicedb-grpc-tonic.sh` from the repo root.

## Usage

```bash
cargo add spicedb-grpc-tonic
```

Use the re-exported Authzed API v1 types and clients:

```rust
use spicedb_grpc_tonic::v1::{
    CheckPermissionRequest,
    PermissionsServiceClient,
    Relationship,
};
```

## Regenerating the generated code

Only needed if you change the proto imports or want to update from a new authzed/api version. From the repo root, with [buf](https://buf.build/docs/installation) installed:

```bash
./scripts/regenerate-spicedb-grpc-tonic.sh
```

Then commit `rust/spicedb-grpc-tonic/src/generated/`.

## License

Apache-2.0
