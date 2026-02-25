"""Google Docs–style example: GET /drive/{folder}/{document_id} with X-User header."""

from contextlib import asynccontextmanager

from authzed.api.v1 import (
    CheckPermissionRequest,
    CheckPermissionResponse,
    Consistency,
    ObjectReference,
    Relationship,
    RelationshipUpdate,
    SubjectReference,
    WriteRelationshipsRequest,
)
from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse

from .schema import DRIVE_SCHEMA


def rel(resource: str, relation: str, subject: str) -> Relationship:
    res_type, res_id = resource.split(":")
    sub_type, sub_id = subject.split(":")
    return Relationship(
        resource=ObjectReference(object_type=res_type, object_id=res_id),
        relation=relation,
        subject=SubjectReference(
            object=ObjectReference(object_type=sub_type, object_id=sub_id)
        ),
    )


def seed_relationships() -> list[Relationship]:
    return [
        rel("document:doc1", "folder", "folder:marketing"),
        rel("document:doc2", "folder", "folder:marketing"),
        rel("document:spec", "folder", "folder:engineering"),
        rel("folder:engineering", "parent", "folder:root"),
        rel("folder:marketing", "parent", "folder:root"),
    ]


def create_app(initial_relationships: list[Relationship] | None = None):
    from spicedb_embedded import EmbeddedSpiceDB

    relationships = (
        initial_relationships
        if initial_relationships is not None
        else seed_relationships()
    )
    spicedb = EmbeddedSpiceDB.start(DRIVE_SCHEMA, relationships)

    @asynccontextmanager
    async def lifespan(app: FastAPI):
        yield
        if getattr(app.state, "spicedb", None) is not None:
            app.state.spicedb.close()

    app = FastAPI(title="Google Docs example", lifespan=lifespan)
    app.state.spicedb = spicedb

    @app.get("/drive/{folder_id}/{document_id}")
    def drive(
        request: Request,
        folder_id: str,
        document_id: str,
    ):
        user = (request.headers.get("x-user") or "").strip()
        if not user:
            return JSONResponse(
                status_code=401,
                content={"error": "Missing X-User header"},
            )

        spicedb = request.app.state.spicedb
        req = CheckPermissionRequest(
            consistency=Consistency(fully_consistent=True),
            resource=ObjectReference(object_type="document", object_id=document_id),
            permission="view",
            subject=SubjectReference(
                object=ObjectReference(object_type="user", object_id=user)
            ),
        )
        try:
            resp = spicedb.permissions().CheckPermission(req)
        except Exception as e:
            return JSONResponse(
                status_code=500,
                content={"error": "Internal error", "message": str(e)},
            )

        if resp.permissionship != CheckPermissionResponse.PERMISSIONSHIP_HAS_PERMISSION:
            return JSONResponse(
                status_code=403,
                content={
                    "error": "Forbidden",
                    "message": f"User {user} does not have view access to document {document_id} in folder {folder_id}",
                },
            )

        return {
            "ok": True,
            "user": user,
            "folder": folder_id,
            "document": document_id,
            "message": "Access allowed",
        }

    return app, spicedb


def main():
    import uvicorn
    app, spicedb = create_app()
    try:
        uvicorn.run(app, host="0.0.0.0", port=3000)
    finally:
        spicedb.close()
