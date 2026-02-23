# Google Docs–style example (C#)

Minimal API app that uses embedded SpiceDB to enforce folder/document permissions. Same behavior as the [Node example](../../node/google-docs/).

## Schema

- **folder**: `parent`, `viewer`, `editor`; permissions `view` and `edit` (including parent inheritance).
- **document**: `folder`, `viewer`, `editor`; permissions `view` and `edit` (from folder and direct).

## API

- **GET** `/drive/{folder}/{documentId}`  
  - Header **`X-User: <userId>`** (e.g. `alice`).  
  - **200**: `{ "ok": true, "user", "folder", "document", "message" }`  
  - **403**: `{ "error": "Forbidden", "message": "..." }`  
  - **401**: `{ "error": "Missing X-User header" }`

## Prerequisites

- .NET 9 SDK
- SpiceDB native library: build from repo root (see main README) or set `SPICEDB_LIBRARY_PATH` to the shared C library.

## Build and run

From this directory (or repo root):

```bash
# Run the example (listens on http://localhost:3000)
dotnet run --project GoogleDocsExample.csproj
```

Try:

```bash
curl -H 'X-User: alice' http://localhost:3000/drive/marketing/doc1
```

## Tests

From this directory:

```bash
dotnet test
```

This builds both the example and the test project, then runs tests. Tests start the app on a random port, set up relationships via the embedded SpiceDB, then call the endpoint with `HttpClient` and assert status and JSON (same scenarios as the Node example).
