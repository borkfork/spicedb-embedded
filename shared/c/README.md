# SpiceDB CGO Library

This Go package builds a C-shared library (`libspicedb.so` / `libspicedb.dylib`) that embeds SpiceDB for use via FFI from Rust or other languages.

## Architecture

This implementation uses SpiceDB's `server` package with an in-memory datastore and Unix socket:

1. **In-memory datastore** - No external database required
2. **Unix socket** - Each instance listens on a unique Unix socket path
3. **Instance-based** - Each call to `spicedb_new` creates an independent server

The Rust side then connects using native **tonic gRPC** over the Unix socket, giving you:
- Production-like API (same as a real SpiceDB server)
- Native Rust protobufs (no JSON serialization overhead)
- **Parallel testing** - each test can have its own isolated instance
- Full gRPC compatibility without network overhead

Based on [authzed/examples/spicedb-as-library](https://github.com/authzed/examples/tree/main/spicedb-as-library).

## Building

```bash
# Build shared library
CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.so .

# On macOS, produces libspicedb.dylib instead
CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.dylib .
```

This generates:
- `libspicedb.so` (Linux) or `libspicedb.dylib` (macOS) - the shared library
- `libspicedb.h` - C header file with function declarations

## Exported Functions

All functions return a JSON string that must be freed with `spicedb_free()`.

### `spicedb_start(options_json) -> handle + streaming_address`

Create a new SpiceDB instance (in-memory; empty server). Schema and relationships should be written by the caller via gRPC.

- `options_json`: Optional pointer to JSON string. Use `NULL` for defaults.
  - **datastore**: `"memory"` (default), `"postgres"`, `"cockroachdb"`, `"spanner"`, `"mysql"`
  - **datastore_uri**: Connection string (required for postgres, cockroachdb, spanner, mysql). E.g. `postgres://user:pass@localhost:5432/spicedb`
  - **spanner_credentials_file**: Path to service account JSON (Spanner only; omit for ADC)
  - **spanner_emulator_host**: e.g. `localhost:9010` (Spanner emulator)
  - **mysql_table_prefix**: Prefix for all tables (MySQL only, optional)
  - **metrics_enabled**: Enable datastore Prometheus metrics (default: false; disabled allows multiple instances in same process)

Returns: `{"success": true, "data": {"handle": 123, "grpc_transport": "memory", "streaming_address": "...", "streaming_transport": "unix"|"tcp"}}`. **streaming_address** is a Unix path when **streaming_transport** is `"unix"`, or `127.0.0.1:port` when `"tcp"`. A streaming proxy is started there for Watch, ReadRelationships, LookupResources, LookupSubjects. Use the handle with RPC FFI for unary calls. If the proxy fails to bind, `spicedb_start` returns an error.

All RPC FFI functions use the same ABI: `(handle, request_bytes, request_len, out_response_bytes, out_response_len, out_error)`. On success, caller frees `*out_response_bytes` with `spicedb_free_bytes`. On error, caller frees `*out_error` with `spicedb_free`.

**PermissionsService:** `spicedb_permissions_check_permission`, `spicedb_permissions_write_relationships`, `spicedb_permissions_delete_relationships`, `spicedb_permissions_check_bulk_permissions`, `spicedb_permissions_expand_permission_tree`

**SchemaService:** `spicedb_schema_read_schema`, `spicedb_schema_write_schema`

Streaming RPCs (Watch, ReadRelationships, LookupResources, LookupSubjects) are not exposed via FFI. For memory transport, use the **`streaming_address`** returned by `spicedb_start` to connect a gRPC client for those APIs.

### `spicedb_free_bytes(ptr)`

Free a byte buffer returned by any RPC FFI function above.

### `spicedb_dispose(handle)`

Dispose of a SpiceDB instance. Frees all resources and removes the socket file.

- `handle`: Instance handle from `spicedb_start`

Returns: `{"success": true}`

### `spicedb_free(ptr)`

Free a string returned by any of the above functions. **Must be called for every returned string.**

## Thread Safety

All exported functions are thread-safe:
- Instance creation/disposal uses locks
- Each instance has its own independent SpiceDB server

Tests can run in parallel since each test creates its own instance!

## Integration with Rust

See `../../rust/` for the Rust FFI bindings that link against this library.

```rust
use spicedb_embedded::EmbeddedSpiceDB;

// Create an instance with schema and relationships
let spicedb = EmbeddedSpiceDB::new(schema, &relationships).await?;

// Uses native tonic gRPC over Unix socket
let allowed = spicedb.check("document:readme", "read", "user:alice").await?;

// Instance is automatically disposed when dropped
```

The Rust client uses native tonic gRPC to communicate with the SpiceDB server,
giving you production-like APIs with minimal overhead.
