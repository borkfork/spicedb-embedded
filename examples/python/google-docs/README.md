# SpiceDB Embedded – Google Docs Example (Python)

This example mimics **basic Google Docs hierarchies**: folders (optionally nested) and documents in a folder, with view/edit permissions enforced by SpiceDB (embedded).

- **Schema:** `folder` (parent, viewer, editor), `document` (folder, viewer, editor). Permissions inherit from folder and parent folder.
- **API:** `GET /drive/{folder_id}/{document_id}` with header **`X-User: <userId>`**. Returns 200 if the user has `view` on the document, 403 otherwise.
- **Tests:** Hit the endpoint with `X-User` via httpx (ASGITransport); they set up relationships with the embedded SpiceDB then assert on status and JSON body.

## Install and run

The example depends on `spicedb-embedded` 0.4.3 from PyPI. From this directory (or repo root):

```bash
pip install -e ".[dev]"   # install example + spicedb-embedded, pytest, httpx
uvicorn google_docs_example.server:create_app --factory --host 0.0.0.0 --port 3000
```

Or run the main helper:

```bash
python -m google_docs_example.server
```

Then:

```bash
curl -i -H 'X-User: alice' http://localhost:3000/drive/marketing/doc1
```

## Tests

```bash
pytest
```

Requires the SpiceDB shared library (e.g. `mise run shared-c-build`).
