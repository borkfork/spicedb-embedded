# spicedb-embedded (Python)

Embedded [SpiceDB](https://authzed.com/spicedb) for Python — authorization server for tests and development. Uses the shared/c C library via ctypes and connects over gRPC via Unix sockets or TCP. Supports memory (default), postgres, cockroachdb, spanner, and mysql datastores.

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
    "grpc_transport": "unix",       # or "tcp"; default by platform
    "datastore_uri": "postgres://user:pass@localhost:5432/spicedb",  # required for remote
    "spanner_credentials_file": "/path/to/key.json",  # Spanner only
    "spanner_emulator_host": "localhost:9010",       # Spanner emulator
    "mysql_table_prefix": "spicedb_",                 # MySQL only (optional)
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
