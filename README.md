# spicedb-embedded

Sometimes you need a simple way to run access checks without spinning up a new service. This library provides an embedded version of [SpiceDB](https://authzed.com/spicedb) in various languages. Each implementation is based on a C-shared library (compiled from the SpiceDB source code) with a very thin FFI binding on top of it.

Communication across the FFI boundary is purposely limited to avoid having to manually manage memory and avoid memory leaks. Instead, when you use the library, it spins up a sidecar server that runs a gRPC service that listens over Unix Sockets (default on Linux/macOS) or TCP (default on Windows).

The default datastore is "memory" (memdb). If you use this datastore, keep in mind that it will reset on each app startup. This is a great option if you can easily provide your schema and relationships at runtime. This way, there are no external network calls to check relationships at runtime.

```typescript
import { v1, EmbeddedSpiceDB } from "spicedb-embedded";

const schema = `definition user {} definition document { relation reader: user permission read = reader }`;
const rel = v1.Relationship.create({
  resource: v1.ObjectReference.create({
    objectType: "document",
    objectId: "readme",
  }),
  relation: "reader",
  subject: v1.SubjectReference.create({
    object: v1.ObjectReference.create({
      objectType: "user",
      objectId: "alice",
    }),
  }),
});

const spicedb = await EmbeddedSpiceDB.create(schema, [rel]);
const resp = await spicedb.permissions().promises.checkPermission(
  v1.CheckPermissionRequest.create({
    resource: v1.ObjectReference.create({
      objectType: "document",
      objectId: "readme",
    }),
    permission: "read",
    subject: v1.SubjectReference.create({
      object: v1.ObjectReference.create({
        objectType: "user",
        objectId: "alice",
      }),
    }),
  }),
);
spicedb.close();
```

If you need a longer term storage, you can use any SpiceDB-compatible datastores.

```typescript
import { EmbeddedSpiceDB } from "spicedb-embedded";

// Run migrations first: spicedb datastore migrate head --datastore-engine postgres --datastore-conn-uri "postgres://..."
const schema = `definition user {} definition document { relation reader: user permission read = reader }`;
const spicedb = await EmbeddedSpiceDB.create(schema, [], {
  datastore: "postgres",
  datastore_uri: "postgres://user:pass@localhost:5432/spicedb",
});
// Use full Permissions API (writeRelationships, checkPermission, etc.)
spicedb.close();
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  Your Application (Rust, Java, Python, C#, TypeScript, etc.)     │
├─────────────────────────────────────────────────────────────────┤
│  Language bindings (rust/, java/, python/, csharp/, node/)        │
│  - FFI/cbindgen to shared/c                                     │
│  - Native gRPC (protobuf) over Unix socket / tcp                │
├─────────────────────────────────────────────────────────────────┤
│  shared/c: C-shared library (Go/CGO)                             │
│  - Embeds SpiceDB server                                        │
│  - Datastore: memory (default), postgres, cockroachdb, spanner, mysql │
│  - Listens on unique Unix socket or TCP per instance            │
└─────────────────────────────────────────────────────────────────┘
```

The **shared/c** library is the foundation—all language bindings build on top of it via C FFI. Each instance is independent, enabling parallel testing. Supports multiple datastores: **memory** (default), **postgres**, **cockroachdb**, **spanner**, and **mysql**.

## Quick Start

Install [mise](https://mise.jdx.dev/installing-mise.html), then run `mise install` to install all tools (Go, Rust, Java, Maven, Python, .NET).

| Language      | README                               | Build & Test           |
| ------------- | ------------------------------------ | ---------------------- |
| **Rust**      | [rust/README.md](rust/README.md)     | `mise run rust-test`   |
| **Java**      | [java/README.md](java/README.md)     | `mise run java-test`   |
| **Python**    | [python/README.md](python/README.md) | `mise run python-test` |
| **C# / .NET** | [csharp/README.md](csharp/README.md) | `mise run csharp-test` |
| **Node.js**   | [node/README.md](node/README.md)     | `mise run node-test`   |

Run all tests: `mise run test`

## Prerequisites

- **Go** 1.23+ with CGO enabled (for building shared/c)
- **Rust** 1.91.1, **Node.js 22**, **Java 17** (Temurin), **Python 3.11**, or **.NET 9** (depending on language) — managed by mise

## Building shared/c

```bash
mise run shared-c-build
# or manually:
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.dylib .  # macOS
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.so .      # Linux
```

## Testing in Docker

To run all tests in a clean Linux environment:

```bash
mise run docker-test
```

Or: `docker build -t spicedb-embedded-test -f Dockerfile .`

## Directory Structure

| Directory   | Description                                                                                                                                                    |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `shared/c/` | Go/CGO library that embeds SpiceDB. Exposes `spicedb_start`, `spicedb_dispose`, `spicedb_free` via C FFI.                                                      |
| `rust/`     | Rust crate — see [rust/README.md](rust/README.md). Thin FFI + [spicedb-grpc](https://docs.rs/spicedb-grpc) clients.                                            |
| `java/`     | Java library — see [java/README.md](java/README.md). JNA FFI + [authzed](https://central.sonatype.com/artifact/com.authzed.api/authzed) gRPC clients.          |
| `python/`   | Python package — see [python/README.md](python/README.md). ctypes FFI + [authzed](https://pypi.org/project/authzed/) gRPC clients.                             |
| `csharp/`   | C# / .NET package — see [csharp/README.md](csharp/README.md). P/Invoke FFI + [Authzed.Net](https://www.nuget.org/packages/Authzed.Net) gRPC clients.           |
| `node/`     | Node.js package — see [node/README.md](node/README.md). koffi FFI + [@authzed/authzed-node](https://www.npmjs.com/package/@authzed/authzed-node) gRPC clients. |

## License

Apache-2.0
