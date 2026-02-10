# spicedb-embedded (C# / .NET)

Sometimes you need a simple way to run access checks without spinning up a new service. This library provides an embedded version of [SpiceDB](https://authzed.com/spicedb) in various languages. Each implementation is based on a C-shared library (compiled from the SpiceDB source code) with a very thin FFI binding on top of it. This means that it runs the native SpiceDB code within your already-running process.

```csharp
using Rendil.Spicedb.Embedded;
using Authzed.Api.V1;

var schema = """
definition user {}

definition document {
    relation reader: user
    permission read = reader
}
""";

var rel = new Relationship
{
    Resource = new ObjectReference { ObjectType = "document", ObjectId = "readme" },
    Relation = "reader",
    Subject = new SubjectReference { Object = new ObjectReference { ObjectType = "user", ObjectId = "alice" } },
};

using var spicedb = EmbeddedSpiceDB.Create(schema, new[] { rel });

var resp = spicedb.Permissions().CheckPermission(new CheckPermissionRequest
{
    Consistency = new Consistency { FullyConsistent = true },
    Resource = new ObjectReference { ObjectType = "document", ObjectId = "readme" },
    Permission = "read",
    Subject = new SubjectReference { Object = new ObjectReference { ObjectType = "user", ObjectId = "alice" } },
});
var allowed = resp.Permissionship == CheckPermissionResponse.Types.Permissionship.HasPermission;
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

```csharp
using Rendil.Spicedb.Embedded;

// Run migrations first: spicedb datastore migrate head --datastore-engine postgres --datastore-conn-uri "postgres://..."
var schema = """
definition user {}

definition document {
    relation reader: user
    permission read = reader
}
""";

using var spicedb = EmbeddedSpiceDB.Create(schema, Array.Empty<Relationship>(), new StartOptions
{
    Datastore = "postgres",
    DatastoreUri = "postgres://user:pass@localhost:5432/spicedb",
});
// Use full Permissions API (WriteRelationships, CheckPermission, etc.)
```

## Running code written in Go compiled to a C-shared library within my service sounds scary

It is scary! Using a C-shared library via FFI bindings introduces memory management in languages that don't typically have to worry about it.

However, this library purposely limits the FFI layer. The only thing it is used for is to spawn the SpiceDB server (and to dispose of it when you shut down the embedded server). Once the SpiceDB server is running, it exposes a gRPC interface that listens over Unix Sockets (default on Linux/macOS) or TCP (default on Windows).

So you get the benefits of (1) using the same generated gRPC code to communicate with SpiceDB that would in a non-embedded world, and (2) communication happens out-of-band so that no memory allocations happen in the FFI layer once the embedded server is running.

## Installation

```bash
dotnet add package SpicedbEmbedded
```

Or from source:

```bash
cd csharp && dotnet add reference ../path/to/SpicedbEmbedded.csproj
```

**Prerequisites:** Go 1.23+ with CGO enabled. Build the shared library first:

```bash
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.dylib .  # macOS
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.so .      # Linux
```

The library looks for `libspicedb.dylib` (macOS) or `libspicedb.so` (Linux) in `SPICEDB_LIBRARY_PATH` or relative to the working directory. Override with `SPICEDB_LIBRARY_PATH=/path/to/shared/c` or `SPICEDB_LIBRARY_PATH=/path/to/libspicedb.dylib`.

**Note:** The shared/c library builds for macOS and Linux only (Unix domain sockets). Windows is not supported for the embedded server.

## Usage

```csharp
using Rendil.Spicedb.Embedded;
using Authzed.Api.V1;

var schema = """
definition user {}

definition document {
    relation reader: user
    permission read = reader
}
""";

var rel = new Relationship
{
    Resource = new ObjectReference { ObjectType = "document", ObjectId = "readme" },
    Relation = "reader",
    Subject = new SubjectReference { Object = new ObjectReference { ObjectType = "user", ObjectId = "alice" } },
};

using var spicedb = EmbeddedSpiceDB.Create(schema, new[] { rel });

var req = new CheckPermissionRequest
{
    Consistency = new Consistency { FullyConsistent = true },
    Resource = new ObjectReference { ObjectType = "document", ObjectId = "readme" },
    Permission = "read",
    Subject = new SubjectReference { Object = new ObjectReference { ObjectType = "user", ObjectId = "alice" } },
};

var resp = spicedb.Permissions().CheckPermission(req);
var allowed = resp.Permissionship == CheckPermissionResponse.Types.Permissionship.HasPermission;
```

## API

- **`EmbeddedSpiceDB.Create(schema, relationships, options?)`** — Create an instance. Pass `null` or `Array.Empty<Relationship>()` for no initial relationships. Pass `StartOptions` for datastore/transport config. Implements `IDisposable`.
- **`Permissions()`** — Permissions service client (CheckPermission, WriteRelationships, ReadRelationships, etc.).
- **`Schema()`** — Schema service client (ReadSchema, WriteSchema, ReflectSchema, etc.).
- **`Watch()`** — Watch service client for relationship changes.
- **`Channel`** — Underlying gRPC channel for custom usage.
- **`Dispose()`** — Dispose the instance and close the channel.

Use types from `Authzed.Api.V1` (ObjectReference, SubjectReference, Relationship, etc.).

### StartOptions

```csharp
var options = new StartOptions
{
    Datastore = "memory",           // or "postgres", "cockroachdb", "spanner", "mysql"
    GrpcTransport = "unix",         // or "tcp"; default by platform
    DatastoreUri = "postgres://...", // required for postgres, cockroachdb, spanner, mysql
    SpannerCredentialsFile = "/path/to/key.json",  // Spanner only
    SpannerEmulatorHost = "localhost:9010",       // Spanner emulator
    MySQLTablePrefix = "spicedb_",                 // MySQL only (optional)
    MetricsEnabled = false,                        // default; set true to enable Prometheus metrics
};

using var spicedb = EmbeddedSpiceDB.Create(schema, relationships, options);
```

## Building & Testing

```bash
mise run shared-c-build
cd csharp
dotnet build
dotnet test
```

Or from the repo root: `mise run csharp-test`
