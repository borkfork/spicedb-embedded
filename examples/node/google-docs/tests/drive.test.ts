import { describe, expect, it } from "vitest";
import request from "supertest";
import { v1 } from "spicedb-embedded";
import { createApp } from "../src/server.js";

const {
  ObjectReference,
  Relationship,
  RelationshipUpdate,
  RelationshipUpdate_Operation,
  SubjectReference,
  WriteRelationshipsRequest,
} = v1;

function rel(resource: string, relation: string, subject: string) {
  const [resType, resId] = resource.split(":");
  const [subType, subId] = subject.split(":");
  return Relationship.create({
    resource: ObjectReference.create({ objectType: resType, objectId: resId }),
    relation,
    subject: SubjectReference.create({
      object: ObjectReference.create({ objectType: subType, objectId: subId }),
    }),
  });
}

describe("Drive permissions (folder → document hierarchy)", () => {
  it("grants user:alice viewer on folder:marketing and verifies alice can view documents in that folder", async () => {
    const { app, spicedb } = await createApp([]);

    try {
      await spicedb.permissions().promises.writeRelationships(
        WriteRelationshipsRequest.create({
          updates: [
            RelationshipUpdate.create({
              operation: RelationshipUpdate_Operation.TOUCH,
              relationship: rel("document:doc1", "folder", "folder:marketing"),
            }),
            RelationshipUpdate.create({
              operation: RelationshipUpdate_Operation.TOUCH,
              relationship: rel("document:doc2", "folder", "folder:marketing"),
            }),
          ],
        }),
      );

      await spicedb.permissions().promises.writeRelationships(
        WriteRelationshipsRequest.create({
          updates: [
            RelationshipUpdate.create({
              operation: RelationshipUpdate_Operation.TOUCH,
              relationship: rel("folder:marketing", "viewer", "user:alice"),
            }),
          ],
        }),
      );

      const res1 = await request(app)
        .get("/drive/marketing/doc1")
        .set("X-User", "alice");
      const res2 = await request(app)
        .get("/drive/marketing/doc2")
        .set("X-User", "alice");

      expect(res1.status).toBe(200);
      expect(res1.body).toMatchObject({
        ok: true,
        user: "alice",
        document: "doc1",
      });
      expect(res2.status).toBe(200);
      expect(res2.body).toMatchObject({
        ok: true,
        user: "alice",
        document: "doc2",
      });
    } finally {
      spicedb.close();
    }
  });

  it("denies user:bob access to documents in folder when bob is not a viewer", async () => {
    const { app, spicedb } = await createApp([
      rel("document:doc1", "folder", "folder:marketing"),
      rel("folder:marketing", "viewer", "user:alice"),
    ]);

    try {
      const res = await request(app)
        .get("/drive/marketing/doc1")
        .set("X-User", "bob");

      expect(res.status).toBe(403);
      expect(res.body.error).toBe("Forbidden");
    } finally {
      spicedb.close();
    }
  });

  it("inherits view from parent folder: user on parent can view documents in child folder", async () => {
    const { app, spicedb } = await createApp([
      rel("folder:marketing", "parent", "folder:root"),
      rel("document:doc1", "folder", "folder:marketing"),
      rel("folder:root", "viewer", "user:alice"),
    ]);

    try {
      const res = await request(app)
        .get("/drive/marketing/doc1")
        .set("X-User", "alice");

      expect(res.status).toBe(200);
      expect(res.body).toMatchObject({
        ok: true,
        user: "alice",
        document: "doc1",
      });
    } finally {
      spicedb.close();
    }
  });

  it("user can be granted permission directly on a document", async () => {
    const { app, spicedb } = await createApp([
      rel("document:doc1", "folder", "folder:marketing"),
      rel("document:doc1", "viewer", "user:alice"),
    ]);

    try {
      const res = await request(app)
        .get("/drive/marketing/doc1")
        .set("X-User", "alice");

      expect(res.status).toBe(200);
      expect(res.body).toMatchObject({
        ok: true,
        user: "alice",
        document: "doc1",
      });
    } finally {
      spicedb.close();
    }
  });

  it("returns 401 when X-User header is missing", async () => {
    const { app, spicedb } = await createApp([
      rel("document:doc1", "folder", "folder:marketing"),
      rel("folder:marketing", "viewer", "user:alice"),
    ]);

    try {
      const res = await request(app).get("/drive/marketing/doc1");

      expect(res.status).toBe(401);
      expect(res.body.error).toBe("Missing X-User header");
    } finally {
      spicedb.close();
    }
  });
});
