# SpiceDB Embedded – Google Docs Example

This example is **meant to mimic basic Google Docs hierarchies**: folders (optionally nested) and documents that live in a folder, with view/edit permissions that flow from folder to document and from parent folder to child folder.

A small Express server exposes a **drive** API with these permissions enforced by SpiceDB (embedded, in-process).

## Schema

- **folder**: Optional `parent` (folder), `viewer` and `editor` (user). Permissions `view` and `edit` inherit from the parent folder.
- **document**: Belongs to one `folder`. `view` / `edit` are derived from the folder’s `view` / `edit` (or from direct viewer/editor on the document).

## API

- **`GET /drive/:folder/:documentId`**  
  Requires header **`X-User: <userId>`**. Returns 200 if the user has `view` on the document, 403 otherwise.

## Run

```bash
npm install
npm run build
npm start
```

Then:

```bash
# No permission
curl -i -H 'X-User: alice' http://localhost:3000/drive/marketing/doc1

# After granting alice viewer on folder:marketing (e.g. via tests or admin), 200
curl -i -H 'X-User: alice' http://localhost:3000/drive/marketing/doc1
```

## Tests

Unit tests hit the Express `GET /drive/:folder/:documentId` endpoint with the `X-User` header to simulate the requesting user. They set up permissions via the embedded SpiceDB (for test data), then assert on HTTP status and response body (200 vs 403, etc.):

- Grant `user:alice` viewer on `folder:marketing` and assert alice can view documents in that folder.
- Assert `user:bob` is denied (403) when not a viewer.
- Assert view permission inherits from parent folder (viewer on `folder:root` can view documents in child `folder:marketing`).
- Assert a user can be granted permission directly on a document.
- Assert 401 when `X-User` header is missing.

```bash
npm test
```
