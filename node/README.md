# spicedb-embedded

Sometimes you need a simple way to run access checks without spinning up a new service. This library provides an embedded version of [SpiceDB](https://authzed.com/spicedb) in various languages. Each implementation is based on a C-shared library (compiled from the SpiceDB source code) with a very thin FFI binding on top of it. This means that it runs the native SpiceDB code within your already-running process.

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
  })
);
spicedb.close();
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

## Running code written in Go compiled to a C-shared library within my service sounds scary

It is scary! Using a C-shared library via FFI bindings introduces memory management in languages that don't typically have to worry about it.

However, this library purposely limits the FFI layer. The only thing it is used for is to spawn the SpiceDB server (and to dispose of it when you shut down the embedded server). Once the SpiceDB server is running, it exposes a gRPC interface that listens over Unix Sockets (default on Linux/macOS) or TCP (default on Windows).

So you get the benefits of (1) using the same generated gRPC code to communicate with SpiceDB that would in a non-embedded world, and (2) communication happens out-of-band so that no memory allocations happen in the FFI layer once the embedded server is running.

## Comparison with SpiceDB WASM

SpiceDB also provides a [WebAssembly (WASM) build](https://authzed.com/blog/some-assembly-required) used in the [Authzed Playground](https://play.authzed.com).

The WASM only provides a small subset of the functionality of SpiceDB, but it has the advantage of working within the context of a browser.

**When to use this package:** Node.js applications that want access to full set of APIs that SpiceDB offers.

**When to use WASM:** Browser-based tools.

## Installation

```bash
npm install spicedb-embedded
```

Or from source (requires Go with CGO; build and test use the same prebuilds layout as the published package):

```bash
mise run shared-c-build
./scripts/stage-all-prebuilds.sh
cd node && npm install && npm run build && npm test
```

Override the library location with `SPICEDB_LIBRARY_PATH` if needed. To test the exact tarball you would publish: `cd node && npm pack`, then install the resulting `.tgz` in another directory.

## Usage

```typescript
import { v1, EmbeddedSpiceDB } from "spicedb-embedded";

const {
  ObjectReference,
  Relationship,
  SubjectReference,
  CheckPermissionRequest,
  Consistency,
  CheckPermissionResponse,
  CheckPermissionResponse_Permissionship,
} = v1;

const schema = `
definition user {}

definition document {
    relation reader: user
    permission read = reader
}
`;

const rel = Relationship.create({
  resource: ObjectReference.create({
    objectType: "document",
    objectId: "readme",
  }),
  relation: "reader",
  subject: SubjectReference.create({
    object: ObjectReference.create({ objectType: "user", objectId: "alice" }),
  }),
});

const spicedb = await EmbeddedSpiceDB.create(schema, [rel]);

try {
  const response = await spicedb.permissions().promises.checkPermission(
    CheckPermissionRequest.create({
      consistency: Consistency.create({
        requirement: { oneofKind: "fullyConsistent", fullyConsistent: true },
      }),
      resource: ObjectReference.create({
        objectType: "document",
        objectId: "readme",
      }),
      permission: "read",
      subject: SubjectReference.create({
        object: ObjectReference.create({
          objectType: "user",
          objectId: "alice",
        }),
      }),
    })
  );

  const allowed =
    response.permissionship ===
    CheckPermissionResponse_Permissionship.HAS_PERMISSION;
} finally {
  spicedb.close();
}
```

## API

- **`EmbeddedSpiceDB.create(schema, relationships, options?)`** — Create an instance (async). Pass `[]` for no initial relationships. Pass `SpiceDBStartOptions` for datastore/transport config.
- **`permissions()`** — Permissions service client (CheckPermission, WriteRelationships, ReadRelationships, etc.). Use `.promises` for async/await.
- **`schema()`** — Schema service client (ReadSchema, WriteSchema, ReflectSchema, etc.).
- **`watch()`** — Watch service client for relationship changes.
- **`close()`** — Dispose the instance and close the channel.

Use types from `v1` (re-exported from `@authzed/authzed-node`) for `ObjectReference`, `SubjectReference`, `Relationship`, etc.

### SpiceDBStartOptions

```typescript
import { EmbeddedSpiceDB, SpiceDBStartOptions } from "spicedb-embedded";

const options: SpiceDBStartOptions = {
  datastore: "memory", // or "postgres", "cockroachdb", "spanner", "mysql"
  grpc_transport: "unix", // or "tcp"; default by platform
  datastore_uri: "postgres://user:pass@localhost:5432/spicedb", // required for remote
  spanner_credentials_file: "/path/to/key.json", // Spanner only
  spanner_emulator_host: "localhost:9010", // Spanner emulator
  mysql_table_prefix: "spicedb_", // MySQL only (optional)
  metrics_enabled: false, // default; set true to enable Prometheus metrics
};

const spicedb = await EmbeddedSpiceDB.create(schema, [], options);
```

## Building & Testing

```bash
mise run shared-c-build
cd node
npm install
npm run build
npm test
```

Or from the repo root: `mise run node-test`
