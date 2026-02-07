import { describe, expect, it } from "vitest";
import { v1, EmbeddedSpiceDB } from "../src/index.js";

const {
  CheckPermissionRequest,
  CheckPermissionResponse,
  CheckPermissionResponse_Permissionship,
  Consistency,
  ObjectReference,
  Relationship,
  RelationshipUpdate,
  RelationshipUpdate_Operation,
  SubjectReference,
  WriteRelationshipsRequest,
} = v1;

function rel(
  resource: string,
  relation: string,
  subject: string
): Relationship {
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

const TEST_SCHEMA = `
definition user {}

definition document {
    relation reader: user
    relation writer: user

    permission read = reader + writer
    permission write = writer
}
`;

const HAS_PERMISSION = CheckPermissionResponse_Permissionship.HAS_PERMISSION;

describe("EmbeddedSpiceDB", () => {
  it("checks permission with initial relationships", async () => {
    const relationships = [
      rel("document:readme", "reader", "user:alice"),
      rel("document:readme", "writer", "user:bob"),
    ];

    const spicedb = await EmbeddedSpiceDB.create(TEST_SCHEMA, relationships);

    try {
      const consistency = Consistency.create({
        requirement: { oneofKind: "fullyConsistent", fullyConsistent: true },
      });
      const checkReq = CheckPermissionRequest.create({
        consistency,
        resource: ObjectReference.create({
          objectType: "document",
          objectId: "readme",
        }),
        permission: "read",
        subject: SubjectReference.create({
          object: ObjectReference.create({
            objectType: "user",
            objectId: "alice",
          }),
        }),
      });

      const response = await spicedb
        .permissions()
        .promises.checkPermission(checkReq);

      expect(response.permissionship).toBe(HAS_PERMISSION);
    } finally {
      spicedb.close();
    }
  });

  it("adds relationship after creation", async () => {
    const spicedb = await EmbeddedSpiceDB.create(TEST_SCHEMA, []);

    try {
      await spicedb.permissions().promises.writeRelationships(
        WriteRelationshipsRequest.create({
          updates: [
            RelationshipUpdate.create({
              operation: RelationshipUpdate_Operation.TOUCH,
              relationship: rel("document:test", "reader", "user:alice"),
            }),
          ],
        })
      );

      const checkReq = CheckPermissionRequest.create({
        consistency: Consistency.create({
          requirement: { oneofKind: "fullyConsistent", fullyConsistent: true },
        }),
        resource: ObjectReference.create({
          objectType: "document",
          objectId: "test",
        }),
        permission: "read",
        subject: SubjectReference.create({
          object: ObjectReference.create({
            objectType: "user",
            objectId: "alice",
          }),
        }),
      });

      const response = await spicedb
        .permissions()
        .promises.checkPermission(checkReq);

      expect(response.permissionship).toBe(HAS_PERMISSION);
    } finally {
      spicedb.close();
    }
  });
});
