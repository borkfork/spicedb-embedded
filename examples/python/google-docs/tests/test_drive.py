"""Tests hit the /drive/{folder}/{document_id} endpoint with X-User header."""

import pytest
from authzed.api.v1 import RelationshipUpdate, WriteRelationshipsRequest
from httpx import ASGITransport, AsyncClient

from google_docs_example import create_app, rel


@pytest.mark.asyncio
async def test_alice_viewer_can_view_documents():
    app, spicedb = create_app([])
    try:
        spicedb.permissions().WriteRelationships(
            WriteRelationshipsRequest(
                updates=[
                    RelationshipUpdate(
                        operation=RelationshipUpdate.Operation.OPERATION_TOUCH,
                        relationship=rel("document:doc1", "folder", "folder:marketing"),
                    ),
                    RelationshipUpdate(
                        operation=RelationshipUpdate.Operation.OPERATION_TOUCH,
                        relationship=rel("document:doc2", "folder", "folder:marketing"),
                    ),
                ]
            )
        )
        spicedb.permissions().WriteRelationships(
            WriteRelationshipsRequest(
                updates=[
                    RelationshipUpdate(
                        operation=RelationshipUpdate.Operation.OPERATION_TOUCH,
                        relationship=rel("folder:marketing", "viewer", "user:alice"),
                    ),
                ]
            )
        )

        async with AsyncClient(
            transport=ASGITransport(app=app),
            base_url="http://test",
        ) as client:
            r1 = await client.get(
                "/drive/marketing/doc1",
                headers={"X-User": "alice"},
            )
            r2 = await client.get(
                "/drive/marketing/doc2",
                headers={"X-User": "alice"},
            )

        assert r1.status_code == 200
        assert r1.json()["ok"] is True
        assert r1.json()["user"] == "alice"
        assert r1.json()["document"] == "doc1"
        assert r2.status_code == 200
        assert r2.json()["document"] == "doc2"
    finally:
        spicedb.close()


@pytest.mark.asyncio
async def test_bob_denied_when_not_viewer():
    app, spicedb = create_app([
        rel("document:doc1", "folder", "folder:marketing"),
        rel("folder:marketing", "viewer", "user:alice"),
    ])
    try:
        async with AsyncClient(
            transport=ASGITransport(app=app),
            base_url="http://test",
        ) as client:
            r = await client.get(
                "/drive/marketing/doc1",
                headers={"X-User": "bob"},
            )

        assert r.status_code == 403
        assert r.json()["error"] == "Forbidden"
    finally:
        spicedb.close()


@pytest.mark.asyncio
async def test_inherits_view_from_parent_folder():
    app, spicedb = create_app([
        rel("folder:marketing", "parent", "folder:root"),
        rel("document:doc1", "folder", "folder:marketing"),
        rel("folder:root", "viewer", "user:alice"),
    ])
    try:
        async with AsyncClient(
            transport=ASGITransport(app=app),
            base_url="http://test",
        ) as client:
            r = await client.get(
                "/drive/marketing/doc1",
                headers={"X-User": "alice"},
            )

        assert r.status_code == 200
        assert r.json()["ok"] is True
    finally:
        spicedb.close()


@pytest.mark.asyncio
async def test_401_when_x_user_missing():
    app, spicedb = create_app([
        rel("document:doc1", "folder", "folder:marketing"),
        rel("folder:marketing", "viewer", "user:alice"),
    ])
    try:
        async with AsyncClient(
            transport=ASGITransport(app=app),
            base_url="http://test",
        ) as client:
            r = await client.get("/drive/marketing/doc1")

        assert r.status_code == 401
        assert r.json()["error"] == "Missing X-User header"
    finally:
        spicedb.close()
