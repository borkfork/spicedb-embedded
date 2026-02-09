---
spicedb-embedded: minor
---

# Add shared datastore integration tests for postgres, cockroachdb, mysql, spanner

Add Rust integration tests that run two embedded SpiceDB servers against the same remote datastore (postgres, cockroachdb, mysql, spanner) and verify writes from one server are visible to the other. Uses testcontainers for databases and the spicedb CLI for migrations. Spanner uses the roryq/spanner-emulator image, which creates the instance and database on startup.
