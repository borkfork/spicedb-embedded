# spicedb-embedded (C# / .NET)

Embedded [SpiceDB](https://authzed.com/spicedb) for .NET — in-memory authorization server for tests and development. Uses the shared/c C library via P/Invoke and connects over gRPC via Unix sockets.

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

- **`EmbeddedSpiceDB.Create(schema, relationships)`** — Create an instance. Pass `null` or `Array.Empty<Relationship>()` for no initial relationships. Implements `IDisposable`.
- **`Permissions()`** — Permissions service client (CheckPermission, WriteRelationships, ReadRelationships, etc.).
- **`Schema()`** — Schema service client (ReadSchema, WriteSchema, ReflectSchema, etc.).
- **`Watch()`** — Watch service client for relationship changes.
- **`Channel`** — Underlying gRPC channel for custom usage.
- **`Dispose()`** — Dispose the instance and close the channel.

Use types from `Authzed.Api.V1` (ObjectReference, SubjectReference, Relationship, etc.).

## Building & Testing

```bash
mise run shared-c-build
cd csharp
dotnet build
dotnet test
```

Or from the repo root: `mise run csharp-test`
