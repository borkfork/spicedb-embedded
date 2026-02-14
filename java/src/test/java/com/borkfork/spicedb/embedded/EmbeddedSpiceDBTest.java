package com.borkfork.spicedb.embedded;

import static org.junit.jupiter.api.Assertions.assertEquals;

import com.authzed.api.v1.CheckPermissionRequest;
import com.authzed.api.v1.CheckPermissionResponse;
import com.authzed.api.v1.Consistency;
import com.authzed.api.v1.ObjectReference;
import com.authzed.api.v1.ReadRelationshipsRequest;
import com.authzed.api.v1.ReadRelationshipsResponse;
import com.authzed.api.v1.Relationship;
import com.authzed.api.v1.RelationshipFilter;
import com.authzed.api.v1.SubjectReference;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

class EmbeddedSpiceDBTest {

  private static final String TEST_SCHEMA =
      """
            definition user {}

            definition document {
                relation reader: user
                relation writer: user

                permission read = reader + writer
                permission write = writer
            }
            """;

  static Relationship rel(String resource, String relation, String subject) {
    String[] resParts = resource.split(":");
    String[] subParts = subject.split(":");
    return Relationship.newBuilder()
        .setResource(
            ObjectReference.newBuilder()
                .setObjectType(resParts[0])
                .setObjectId(resParts[1])
                .build())
        .setRelation(relation)
        .setSubject(
            SubjectReference.newBuilder()
                .setObject(
                    ObjectReference.newBuilder()
                        .setObjectType(subParts[0])
                        .setObjectId(subParts[1])
                        .build())
                .build())
        .build();
  }

  @Test
  void checkPermission() {
    var relationships =
        List.of(
            rel("document:readme", "reader", "user:alice"),
            rel("document:readme", "writer", "user:bob"));

    try (var spicedb = EmbeddedSpiceDB.create(TEST_SCHEMA, relationships)) {
      var consistency = Consistency.newBuilder().setFullyConsistent(true).build();

      var checkReq =
          CheckPermissionRequest.newBuilder()
              .setConsistency(consistency)
              .setResource(
                  ObjectReference.newBuilder()
                      .setObjectType("document")
                      .setObjectId("readme")
                      .build())
              .setPermission("read")
              .setSubject(
                  SubjectReference.newBuilder()
                      .setObject(
                          ObjectReference.newBuilder()
                              .setObjectType("user")
                              .setObjectId("alice")
                              .build())
                      .build())
              .build();

      var response = spicedb.permissions().checkPermission(checkReq);
      assertEquals(
          CheckPermissionResponse.Permissionship.PERMISSIONSHIP_HAS_PERMISSION,
          response.getPermissionship());

      // Alice has read, not write
      var writeReq = checkReq.toBuilder().setPermission("write").build();
      var writeResp = spicedb.permissions().checkPermission(writeReq);
      assertEquals(
          CheckPermissionResponse.Permissionship.PERMISSIONSHIP_NO_PERMISSION,
          writeResp.getPermissionship());

      // Bob has read and write
      var bobReadReq =
          checkReq.toBuilder()
              .setSubject(
                  SubjectReference.newBuilder()
                      .setObject(
                          ObjectReference.newBuilder()
                              .setObjectType("user")
                              .setObjectId("bob")
                              .build())
                      .build())
              .build();
      var bobReadResp = spicedb.permissions().checkPermission(bobReadReq);
      assertEquals(
          CheckPermissionResponse.Permissionship.PERMISSIONSHIP_HAS_PERMISSION,
          bobReadResp.getPermissionship());
    }
  }

  @Test
  void addRelationship() {
    try (var spicedb = EmbeddedSpiceDB.create(TEST_SCHEMA, List.of())) {
      spicedb
          .permissions()
          .writeRelationships(
              com.authzed.api.v1.WriteRelationshipsRequest.newBuilder()
                  .addUpdates(
                      com.authzed.api.v1.RelationshipUpdate.newBuilder()
                          .setOperation(
                              com.authzed.api.v1.RelationshipUpdate.Operation.OPERATION_TOUCH)
                          .setRelationship(rel("document:test", "reader", "user:alice"))
                          .build())
                  .build());

      var checkReq =
          CheckPermissionRequest.newBuilder()
              .setConsistency(Consistency.newBuilder().setFullyConsistent(true).build())
              .setResource(
                  ObjectReference.newBuilder()
                      .setObjectType("document")
                      .setObjectId("test")
                      .build())
              .setPermission("read")
              .setSubject(
                  SubjectReference.newBuilder()
                      .setObject(
                          ObjectReference.newBuilder()
                              .setObjectType("user")
                              .setObjectId("alice")
                              .build())
                      .build())
              .build();

      var response = spicedb.permissions().checkPermission(checkReq);
      assertEquals(
          CheckPermissionResponse.Permissionship.PERMISSIONSHIP_HAS_PERMISSION,
          response.getPermissionship());
    }
  }

  @Test
  void readRelationshipsStreaming() {
    var relationships =
        List.of(
            rel("document:doc1", "reader", "user:alice"),
            rel("document:doc1", "writer", "user:bob"),
            rel("document:doc2", "reader", "user:alice"));

    try (var spicedb = EmbeddedSpiceDB.create(TEST_SCHEMA, relationships)) {
      var req =
          ReadRelationshipsRequest.newBuilder()
              .setConsistency(Consistency.newBuilder().setFullyConsistent(true).build())
              .setRelationshipFilter(
                  RelationshipFilter.newBuilder()
                      .setResourceType("document")
                      .setOptionalResourceId("doc1")
                      .build())
              .build();

      var results = new ArrayList<ReadRelationshipsResponse>();
      spicedb.permissions().readRelationships(req).forEachRemaining(results::add);

      assertEquals(2, results.size());
    }
  }
}
