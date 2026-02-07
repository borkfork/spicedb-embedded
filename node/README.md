# spicedb-embedded (Node.js)

Embedded [SpiceDB](https://authzed.com/spicedb) for Node.js — in-memory authorization server for tests and development. Uses the shared/c C library via koffi FFI and connects over gRPC via Unix sockets. **Node.js only** (not for browser/frontend).

## Comparison with SpiceDB WASM

SpiceDB also provides a [WebAssembly (WASM) build](https://authzed.com/blog/some-assembly-required) used in the [Authzed Playground](https://play.authzed.com). This Node.js implementation differs in several important ways:

| Aspect           | **spicedb-embedded** (this package)                                  | **SpiceDB WASM**                                       |
| ---------------- | -------------------------------------------------------------------- | ------------------------------------------------------ |
| **Runtime**      | Node.js only (native addon)                                          | Browser + Node.js (via WASM)                           |
| **Architecture** | Spawns real SpiceDB server via C/CGO FFI; full gRPC over Unix socket | Compiled Go→WASM; callback-based development API       |
| **API**          | Full SpiceDB gRPC API (Permissions, Schema, Watch, etc.)             | Development API only (check, validate, run operations) |
| **Use case**     | Server-side tests, development, local tooling                        | Browser Playground, client-side validation             |
| **Performance**  | Native SpiceDB; full caching, production-grade                       | No Ristretto cache; simplified for WASM                |
| **Distribution** | Requires platform-specific `libspicedb.so`/`.dylib`                  | Single `.wasm` file (~25MB)                            |

**When to use this package:** Node.js applications that need an embedded SpiceDB for integration tests, local development, or tooling. You get the real SpiceDB implementation with the full API.

**When to use WASM:** Browser-based tools (e.g., schema playground) or when you need a single portable binary.

## Installation

```bash
npm install spicedb-embedded
```

Or from source:

```bash
cd node && npm install
```

**Prerequisites:** Go 1.23+ with CGO enabled. Build the shared library first:

```bash
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.dylib .  # macOS
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.so .      # Linux
```

The library looks for `libspicedb.dylib` (macOS) or `libspicedb.so` (Linux) in `SPICEDB_LIBRARY_PATH` or relative to the working directory. Override with `SPICEDB_LIBRARY_PATH=/path/to/shared/c`.

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

- **`EmbeddedSpiceDB.create(schema, relationships)`** — Create an instance (async). Pass `[]` for no initial relationships.
- **`permissions()`** — Permissions service client (CheckPermission, WriteRelationships, ReadRelationships, etc.). Use `.promises` for async/await.
- **`schema()`** — Schema service client (ReadSchema, WriteSchema, ReflectSchema, etc.).
- **`watch()`** — Watch service client for relationship changes.
- **`close()`** — Dispose the instance and close the channel.

Use types from `v1` (re-exported from `@authzed/authzed-node`) for `ObjectReference`, `SubjectReference`, `Relationship`, etc.

## Building & Testing

```bash
mise run shared-c-build
cd node
npm install
npm run build
npm test
```

Or from the repo root: `mise run node-test`
