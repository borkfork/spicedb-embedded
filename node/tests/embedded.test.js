import { describe, expect, it } from "vitest";
import {
  CheckPermissionRequest,
  CheckPermissionResponse,
  Consistency,
  EmbeddedSpiceDB,
  ObjectReference,
  Relationship,
  SubjectReference,
} from "../src/index.js";
import {
  RelationshipUpdate,
  WriteRelationshipsRequest,
} from "@authzed/authzed-node";
function rel(resource, relation, subject) {
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
const HAS_PERMISSION =
  CheckPermissionResponse.Permissionship.PERMISSIONSHIP_HAS_PERMISSION;
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
              operation: RelationshipUpdate.Operation.OPERATION_TOUCH,
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
//# sourceMappingURL=embedded.test.js.map
