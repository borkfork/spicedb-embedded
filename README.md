<div align="center">

![GitHub License](https://img.shields.io/github/license/borkfork/spicedb-embedded?style=plastic)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/borkfork/spicedb-embedded/ci.yml?branch=main&style=plastic&label=CI)
![GitHub Release](https://img.shields.io/github/v/release/borkfork/spicedb-embedded?filter=!*grpc*&style=plastic)

</div>

# spicedb-embedded - SpiceDB as a library

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

const spicedb = await EmbeddedSpiceDB.start(schema, [rel]);
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

## Who should consider using this?

If you want to spin up SpiceDB, but don't want the overhead of managing another service, this might be for you.

If you want an embedded server for your unit tests and don't have access to Docker / Testcontainers, this might be for you.

If you have a schema and set of permissions that are static / readonly, this might be for you.

## Who should avoid using this?

If you live in a world of microservices that each need to perform permission checks, you should almost certainly spin up a centralized SpiceDB deployment.

## How does storage work?

The default datastore is "memory" (memdb). If you use this datastore, keep in mind that it will reset on each app startup. This is a great option if you can easily provide your schema and relationships at runtime. This way, there are no external network calls to check relationships at runtime.

If you need a longer term storage, you can use any SpiceDB-compatible datastores.

```typescript
import { EmbeddedSpiceDB } from "spicedb-embedded";

// Run migrations first: spicedb datastore migrate head --datastore-engine postgres --datastore-conn-uri "postgres://..."
const schema = `definition user {} definition document { relation reader: user permission read = reader }`;
const spicedb = await EmbeddedSpiceDB.start(schema, [], {
  datastore: "postgres",
  datastore_uri: "postgres://user:pass@localhost:5432/spicedb",
});
// Use full Permissions API (writeRelationships, checkPermission, etc.)
spicedb.close();
```

## Running code written in Go compiled to a C-shared library within my service sounds scary

It is scary! Using a C-shared library via FFI bindings introduces memory management in languages that don't typically have to worry about it.

That being said, the SpiceDB code still runs in a Go runtime with garbage collection, which is where the vast majority of time is spent. To help mitigate some of the risk, the FFI layer is kept as straightforward as possible. protobuf is marshalled and unmarshalled at the FFI <--> language runtime boundary in a standardized way. After unmarshalling, requests are sent directly to the SpiceDB server, and responses are returned directly back to the language runtime (after marshalling).

For architecture details, build setup, and contributing guidelines, see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache-2.0
