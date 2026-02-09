# spicedb-embedded (Java)

Embedded [SpiceDB](https://authzed.com/spicedb) for the JVM — authorization server for tests and development. Uses the shared/c C library via JNA and connects over gRPC via Unix sockets or TCP. Supports memory (default), postgres, cockroachdb, spanner, and mysql datastores.

## Installation

Add to your `pom.xml`:

```xml
<dependency>
    <groupId>com.rendil</groupId>
    <artifactId>spicedb-embedded</artifactId>
    <version>0.1.0-SNAPSHOT</version>
</dependency>
```

**Prerequisites:** Go 1.23+ with CGO enabled. Build the shared library first:

```bash
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.dylib .  # macOS
cd shared/c && CGO_ENABLED=1 go build -buildmode=c-shared -o libspicedb.so .      # Linux
```

The library looks for `libspicedb.dylib` (macOS) or `libspicedb.so` (Linux) in `java.library.path` or relative to the working directory (`shared/c`, `../shared/c`, etc.). Override with `-Dspicedb.library.path=/path/to/shared/c`.

## Usage

```java
import com.rendil.spicedb.embedded.EmbeddedSpiceDB;
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
