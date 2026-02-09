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

### `spicedb_start(options_json) -> handle + grpc_transport + address`

Create a new SpiceDB instance (empty server). Schema and relationships should be written by the caller via gRPC.

- `options_json`: Optional pointer to JSON string. Use `NULL` for defaults.
  - **datastore**: `"memory"` (default), `"postgres"`, `"cockroachdb"`, `"spanner"`, `"mysql"`
  - **datastore_uri**: Connection string (required for postgres, cockroachdb, spanner, mysql). E.g. `postgres://user:pass@localhost:5432/spicedb`
  - **grpc_transport**: `"unix"` (default on Unix), `"tcp"` (default on Windows)
  - **spanner_credentials_file**: Path to service account JSON (Spanner only; omit for ADC)
  - **spanner_emulator_host**: e.g. `localhost:9010` (Spanner emulator)
  - **mysql_table_prefix**: Prefix for all tables (MySQL only, optional)

Returns:
- Unix: `{"success": true, "data": {"handle": 123, "grpc_transport": "unix", "address": "/tmp/spicedb-xxx.sock"}}`
- Windows: `{"success": true, "data": {"handle": 123, "grpc_transport": "tcp", "address": "127.0.0.1:50051"}}`

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
