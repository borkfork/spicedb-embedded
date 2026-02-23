# SpiceDB Embedded – Google Docs Example (Java)

This example mimics **basic Google Docs hierarchies**: folders (optionally nested) and documents in a folder, with view/edit permissions enforced by SpiceDB (embedded).

- **Schema:** `folder` (parent, viewer, editor), `document` (folder, viewer, editor). Permissions inherit from folder and parent folder.
- **API:** `GET /drive/:folder/:documentId` with header **`X-User: <userId>`**. Returns 200 if the user has `view` on the document, 403 otherwise.
- **Tests:** Hit the endpoint with `X-User` via `HttpClient`; they set up relationships with the embedded SpiceDB then assert on status and JSON body.

## Build and run

The example depends on `spicedb-embedded` 0.4.3 from Maven Central. Build and run:

```bash
mvn package
java -jar target/google-docs-example-0.1.0.jar
```

Or run the main class with classpath:

```bash
mvn compile exec:java -Dexec.mainClass="com.borkfork.spicedb.examples.googledocs.Main"
```

Then:

```bash
curl -i -H 'X-User: alice' http://localhost:3000/drive/marketing/doc1
```

## Tests

```bash
mvn test
```

Requires the SpiceDB shared library (e.g. `mise run shared-c-build`).
