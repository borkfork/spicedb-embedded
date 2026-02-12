# spicedb-embedded (Java)

Sometimes you need a simple way to run access checks without spinning up a new service. This library provides an embedded version of [SpiceDB](https://authzed.com/spicedb) in various languages. Each implementation is based on a C-shared library (compiled from the SpiceDB source code) with a very thin FFI binding on top of it. This means that it runs the native SpiceDB code within your already-running process.

```java
import com.borkfork.spicedb.embedded.EmbeddedSpiceDB;
import com.authzed.api.v1.*;

String schema = """
    definition user {}
    definition document {
        relation reader: user
        permission read = reader
    }
    """;

Relationship rel = Relationship.newBuilder()
    .setResource(ObjectReference.newBuilder()
        .setObjectType("document")
        .setObjectId("readme")
        .build())
    .setRelation("reader")
    .setSubject(SubjectReference.newBuilder()
        .setObject(ObjectReference.newBuilder()
            .setObjectType("user")
            .setObjectId("alice")
            .build())
        .build())
    .build();

try (var spicedb = EmbeddedSpiceDB.create(schema, List.of(rel))) {
    var response = spicedb.permissions().checkPermission(
        CheckPermissionRequest.newBuilder()
            .setConsistency(Consistency.newBuilder().setFullyConsistent(true).build())
            .setResource(ObjectReference.newBuilder()
                .setObjectType("document")
                .setObjectId("readme")
                .build())
            .setPermission("read")
            .setSubject(SubjectReference.newBuilder()
                .setObject(ObjectReference.newBuilder()
                    .setObjectType("user")
                    .setObjectId("alice")
                    .build())
                .build())
            .build()
    );
    boolean allowed = response.getPermissionship()
        == CheckPermissionResponse.Permissionship.PERMISSIONSHIP_HAS_PERMISSION;
}
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

```java
import com.borkfork.spicedb.embedded.EmbeddedSpiceDB;
import com.borkfork.spicedb.embedded.StartOptions;

// Run migrations first: spicedb datastore migrate head --datastore-engine postgres --datastore-conn-uri "postgres://..."
String schema = """
    definition user {}
    definition document {
        relation reader: user
        permission read = reader
    }
    """;

var options = StartOptions.builder()
    .datastore("postgres")
    .datastoreUri("postgres://user:pass@localhost:5432/spicedb")
    .build();

try (var spicedb = EmbeddedSpiceDB.create(schema, List.of(), options)) {
    // Use full Permissions API (writeRelationships, checkPermission, etc.)
}
```

## Running code written in Go compiled to a C-shared library within my service sounds scary

It is scary! Using a C-shared library via FFI bindings introduces memory management in languages that don't typically have to worry about it.

However, this library purposely limits the FFI layer. The only thing it is used for is to spawn the SpiceDB server (and to dispose of it when you shut down the embedded server). Once the SpiceDB server is running, it exposes a gRPC interface that listens over Unix Sockets (default on Linux/macOS) or TCP (default on Windows).

So you get the benefits of (1) using the same generated gRPC code to communicate with SpiceDB that would in a non-embedded world, and (2) communication happens out-of-band so that no memory allocations happen in the FFI layer once the embedded server is running.

## Installation

Add the main library and **one** classifier dependency for your platform (`linux-x64`, `darwin-arm64`, or `win32-x64`) to your `pom.xml`:

```xml
<dependency>
    <groupId>com.borkfork</groupId>
    <artifactId>spicedb-embedded</artifactId>
    <version>0.1.21</version>
</dependency>
<dependency>
    <groupId>com.borkfork</groupId>
    <artifactId>spicedb-embedded</artifactId>
    <version>0.1.21</version>
    <classifier>linux-x64</classifier>  <!-- or darwin-arm64, win32-x64 -->
</dependency>
```

**Without a classifier**, the library falls back to `spicedb.library.path` or `shared/c` (see Prerequisites).

**Prerequisites (only if not using a classifier JAR):** Go 1.23+ with CGO enabled. Build the shared library first:

```bash
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.dylib .  # macOS
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.so .      # Linux
```

The library looks for `libspicedb.dylib` (macOS) or `libspicedb.so` (Linux) in `java.library.path` or relative to the working directory (`shared/c`, `../shared/c`, etc.). Override with `-Dspicedb.library.path=/path/to/shared/c`.

## Usage

```java
import com.borkfork.spicedb.embedded.EmbeddedSpiceDB;
import com.authzed.api.v1.*;

String schema = """
    definition user {}
    definition document {
        relation reader: user
        permission read = reader
    }
    """;

Relationship rel = Relationship.newBuilder()
    .setResource(ObjectReference.newBuilder()
        .setObjectType("document")
        .setObjectId("readme")
        .build())
    .setRelation("reader")
    .setSubject(SubjectReference.newBuilder()
        .setObject(ObjectReference.newBuilder()
            .setObjectType("user")
            .setObjectId("alice")
            .build())
        .build())
    .build();

try (var spicedb = EmbeddedSpiceDB.create(schema, List.of(rel))) {
    var stub = spicedb.permissions();
    var request = CheckPermissionRequest.newBuilder()
        .setConsistency(Consistency.newBuilder().setFullyConsistent(true).build())
        .setResource(ObjectReference.newBuilder()
            .setObjectType("document")
            .setObjectId("readme")
            .build())
        .setPermission("read")
        .setSubject(SubjectReference.newBuilder()
            .setObject(ObjectReference.newBuilder()
                .setObjectType("user")
                .setObjectId("alice")
                .build())
            .build())
        .build();

    var response = stub.checkPermission(request);
    boolean allowed = response.getPermissionship()
        == CheckPermissionResponse.Permissionship.PERMISSIONSHIP_HAS_PERMISSION;
}
```

## API

- **`EmbeddedSpiceDB.create(schema, relationships)`** — Create an instance. Pass `List.of()` for no initial relationships.
- **`EmbeddedSpiceDB.create(schema, relationships, options)`** — Create with options (datastore, `grpc_transport`, etc.). Pass `null` for defaults.
- **`permissions()`** — Blocking stub for CheckPermission, WriteRelationships, ReadRelationships, etc.
- **`schema()`** — Blocking stub for ReadSchema, WriteSchema, ReflectSchema, etc.
- **`watch()`** — Blocking stub for watching relationship changes.
- **`channel()`** — Underlying gRPC channel for custom usage.
- **`close()`** — Implements `AutoCloseable`; dispose the instance and close the channel.

Use types from `com.authzed.api.v1` (ObjectReference, SubjectReference, Relationship, etc.).

### StartOptions

Configure datastore and transport via `StartOptions`:

```java
var options = StartOptions.builder()
    .datastore("memory")           // default; or "postgres", "cockroachdb", "spanner", "mysql"
    .grpcTransport("unix")          // or "tcp"; default by platform
    .datastoreUri("postgres://...")  // required for postgres, cockroachdb, spanner, mysql
    .build();

try (var spicedb = EmbeddedSpiceDB.create(schema, List.of(), options)) {
    // ...
}
```

- **datastore**: `"memory"` (default), `"postgres"`, `"cockroachdb"`, `"spanner"`, `"mysql"`
- **datastore_uri**: Connection string (required for remote datastores)
- **grpc_transport**: `"unix"` (default on Unix), `"tcp"` (default on Windows)
- **spanner_credentials_file**, **spanner_emulator_host**: Spanner-only
- **mysql_table_prefix**: MySQL-only (optional)
- **metrics_enabled**: Enable datastore Prometheus metrics (default: false)

## JVM Warnings

You may see warnings when running your application. These come from JNA (native library loading) and Netty (gRPC transport). To suppress them, add these JVM options when starting your app:

```
--enable-native-access=ALL-UNNAMED
--add-opens=jdk.unsupported/sun.misc=ALL-UNNAMED
```

For **Java 23+**, also add (to suppress Unsafe deprecation warnings):

```
-XX:+IgnoreUnrecognizedVMOptions
--sun-misc-unsafe-memory-access=allow
```

**Examples:**

```bash
# Run your app
java -Djava.library.path=path/to/shared/c \
  --enable-native-access=ALL-UNNAMED \
  --add-opens=jdk.unsupported/sun.misc=ALL-UNNAMED \
  -jar your-app.jar
```

Maven/Gradle: set these in your run configuration or `MAVEN_OPTS` / `GRADLE_OPTS` when running tests.

## Building & Testing

```bash
mise run shared-c-build
cd java
mvn test
```

Or from the repo root: `mise run java-test`
