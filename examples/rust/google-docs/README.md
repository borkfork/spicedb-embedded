# SpiceDB Embedded – Google Docs Example (Rust)

This example mimics **basic Google Docs hierarchies**: folders (optionally nested) and documents in a folder, with view/edit permissions enforced by SpiceDB (embedded).

- **Schema:** `folder` (parent, viewer, editor), `document` (folder, viewer, editor). Permissions inherit from folder and parent folder.
- **API:** `GET /drive/:folder/:document_id` with header **`X-User: <userId>`**. Returns 200 if the user has `view` on the document, 403 otherwise.
- **Tests:** Hit the endpoint with `X-User` via HTTP (reqwest); they set up relationships with the embedded SpiceDB then assert on status and JSON body.

## Build and run

From the **repository root** (this crate is a workspace member):

```bash
cargo build -p google-docs-example
cargo run -p google-docs-example
```

Then:

```bash
curl -i -H 'X-User: alice' http://localhost:3000/drive/marketing/doc1
```

## Tests

From the repository root:

```bash
cargo test -p google-docs-example
```

Requires the SpiceDB shared library to be built (e.g. `mise run shared-c-build` or CI).
