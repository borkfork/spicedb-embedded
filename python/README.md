# spicedb-embedded (Python)

Sometimes you need a simple way to run access checks without spinning up a new service. This library provides an embedded version of [SpiceDB](https://authzed.com/spicedb) in various languages. Each implementation is based on a C-shared library (compiled from the SpiceDB source code) with a very thin FFI binding on top of it. This means that it runs the native SpiceDB code within your already-running process.

```python
from spicedb_embedded import EmbeddedSpiceDB
from authzed.api.v1 import (
    CheckPermissionRequest,
    CheckPermissionResponse,
    Consistency,
    ObjectReference,
    Relationship,
    SubjectReference,
)

schema = """
definition user {}

definition document {
    relation reader: user
    permission read = reader
}
"""

rel = Relationship(
    resource=ObjectReference(object_type="document", object_id="readme"),
    relation="reader",
    subject=SubjectReference(object=ObjectReference(object_type="user", object_id="alice")),
)

with EmbeddedSpiceDB(schema, [rel]) as spicedb:
    req = CheckPermissionRequest(
        consistency=Consistency(fully_consistent=True),
        resource=ObjectReference(object_type="document", object_id="readme"),
        permission="read",
        subject=SubjectReference(object=ObjectReference(object_type="user", object_id="alice")),
    )
    resp = spicedb.permissions().CheckPermission(req)
    allowed = resp.permissionship == CheckPermissionResponse.PERMISSIONSHIP_HAS_PERMISSION
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

```python
from spicedb_embedded import EmbeddedSpiceDB

# Run migrations first: spicedb datastore migrate head --datastore-engine postgres --datastore-conn-uri "postgres://..."
schema = """
definition user {}

definition document {
    relation reader: user
    permission read = reader
}
"""

with EmbeddedSpiceDB(schema, [], options={
    "datastore": "postgres",
    "datastore_uri": "postgres://user:pass@localhost:5432/spicedb",
}) as spicedb:
    # Use full Permissions API (writeRelationships, checkPermission, etc.)
    pass
```

## Running code written in Go compiled to a C-shared library within my service sounds scary

It is scary! Using a C-shared library via FFI bindings introduces memory management in languages that don't typically have to worry about it.

That being said, the SpiceDB code still runs in a Go runtime with garbage collection, which is where the vast majority of time is spent. To help mitigate some of the risk, the FFI layer is kept as straightforward as possible. protobuf is marshalled and unmarshalled at the FFI <--> language runtime boundary in a standardized way. After unmarshalling, requests are sent directly to the SpiceDB server, and responses are returned directly back to the language runtime (after marshalling).

## Installation

```bash
pip install spicedb-embedded
```

Or from source:

```bash
cd python && pip install -e .
```

**Prerequisites:** Go 1.23+ with CGO enabled. Build the shared library first:

```bash
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.dylib .  # macOS
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.so .      # Linux
```

The library looks for `libspicedb.dylib` (macOS) or `libspicedb.so` (Linux) in `SPICEDB_LIBRARY_PATH` or relative to the working directory (`shared/c`, `../shared/c`, etc.). Override with `SPICEDB_LIBRARY_PATH=/path/to/shared/c`.

## Usage

```python
from spicedb_embedded import EmbeddedSpiceDB
from authzed.api.v1 import (
    CheckPermissionRequest,
    CheckPermissionResponse,
    Consistency,
    ObjectReference,
    Relationship,
    SubjectReference,
)

schema = """
definition user {}

definition document {
    relation reader: user
    permission read = reader
}
"""

rel = Relationship(
    resource=ObjectReference(object_type="document", object_id="readme"),
    relation="reader",
    subject=SubjectReference(object=ObjectReference(object_type="user", object_id="alice")),
)

with EmbeddedSpiceDB(schema, [rel]) as spicedb:
    stub = spicedb.permissions()
    req = CheckPermissionRequest(
        consistency=Consistency(fully_consistent=True),
        resource=ObjectReference(object_type="document", object_id="readme"),
        permission="read",
        subject=SubjectReference(object=ObjectReference(object_type="user", object_id="alice")),
    )
    resp = stub.CheckPermission(req)
    allowed = resp.permissionship == CheckPermissionResponse.PERMISSIONSHIP_HAS_PERMISSION
```

## API

- **`EmbeddedSpiceDB(schema, relationships, options=None)`** — Create an instance. Pass `[]` for no initial relationships. Pass `options` dict for datastore/transport config. Supports context manager (`with`).
- **`permissions()`** — Permissions service stub (CheckPermission, WriteRelationships, ReadRelationships, etc.).
- **`schema()`** — Schema service stub (ReadSchema, WriteSchema, ReflectSchema, etc.).
- **`watch()`** — Watch service stub for relationship changes.
- **`channel()`** — Underlying gRPC channel for custom usage.
- **`close()`** — Dispose the instance and close the channel.

Use types from `authzed.api.v1` (ObjectReference, SubjectReference, Relationship, etc.).

### Options

```python
options = {
    "datastore": "memory",          # or "postgres", "cockroachdb", "spanner", "mysql"
    "datastore_uri": "postgres://user:pass@localhost:5432/spicedb",  # required for remote
    "spanner_credentials_file": "/path/to/key.json",  # Spanner only
    "spanner_emulator_host": "localhost:9010",       # Spanner emulator
    "mysql_table_prefix": "spicedb_",                 # MySQL only (optional)
    "metrics_enabled": False,                         # default; set True to enable Prometheus metrics
}

with EmbeddedSpiceDB(schema, [], options=options) as spicedb:
    ...
```

## Building & Testing

```bash
mise run shared-c-build
cd python
pip install -e ".[dev]"
pytest
```

Or from the repo root: `mise run python-test`
