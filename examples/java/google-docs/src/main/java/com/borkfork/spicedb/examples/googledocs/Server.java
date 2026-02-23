package com.borkfork.spicedb.examples.googledocs;

import com.authzed.api.v1.*;
import com.borkfork.spicedb.embedded.EmbeddedSpiceDB;
import io.javalin.Javalin;
import io.javalin.http.Context;
import java.util.List;
import org.slf4j.LoggerFactory;
import org.slf4j.Logger;

/** Google Docs–style server: GET /drive/:folder/:documentId with X-User header. */
public final class Server {
  private static final Logger LOGGER = LoggerFactory.getLogger(Server.class);

  public static Relationship rel(String resource, String relation, String subject) {
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

  private static List<Relationship> seedRelationships() {
    return List.of(
        rel("document:doc1", "folder", "folder:marketing"),
        rel("document:doc2", "folder", "folder:marketing"),
        rel("document:spec", "folder", "folder:engineering"),
        rel("folder:engineering", "parent", "folder:root"),
        rel("folder:marketing", "parent", "folder:root"));
  }

  /** Create app and SpiceDB. Pass null for default seed, or a list for custom (e.g. empty). */
  public static AppAndSpiceDB createApp(List<Relationship> initialRelationships) {
    List<Relationship> relationships =
        initialRelationships != null ? initialRelationships : seedRelationships();
    EmbeddedSpiceDB spicedb = EmbeddedSpiceDB.create(Schema.DRIVE_SCHEMA, relationships);

    Javalin app =
        Javalin.create()
            .get(
                "/drive/{folder}/{documentId}",
                ctx -> drive(ctx, spicedb));

    return new AppAndSpiceDB(app, spicedb);
  }

  public record AppAndSpiceDB(Javalin app, EmbeddedSpiceDB spicedb) {}

  private static void drive(Context ctx, EmbeddedSpiceDB spicedb) {
    String folderId = ctx.pathParam("folder");
    String documentId = ctx.pathParam("documentId");
    String user = ctx.header("X-User");
    if (user == null || user.isBlank()) {
      ctx.status(401).json(java.util.Map.of("error", "Missing X-User header"));
      return;
    }
    user = user.trim();

    var consistency = Consistency.newBuilder().setFullyConsistent(true).build();
    var request =
        CheckPermissionRequest.newBuilder()
            .setConsistency(consistency)
            .setResource(
                ObjectReference.newBuilder()
                    .setObjectType("document")
                    .setObjectId(documentId)
                    .build())
            .setPermission("view")
            .setSubject(
                SubjectReference.newBuilder()
                    .setObject(
                        ObjectReference.newBuilder()
                            .setObjectType("user")
                            .setObjectId(user)
                            .build())
                    .build())
            .build();

    try {
      var response = spicedb.permissions().checkPermission(request);

      LOGGER.info(
          "Permission check result for user '{}' on document '{}': {}",
          user,
          documentId,
          response.getPermissionship());
      if (response.getPermissionship()
          != CheckPermissionResponse.Permissionship.PERMISSIONSHIP_HAS_PERMISSION) {
        ctx.status(403)
            .json(
                new ErrorBody(
                    "Forbidden",
                    "User "
                        + user
                        + " does not have view access to document "
                        + documentId
                        + " in folder "
                        + folderId));
        return;
      }
      ctx.json(
          new SuccessBody(true, user, folderId, documentId, "Access allowed"));
    } catch (Exception e) {
      LOGGER.error("Error checking permissions for user '{}' on document '{}'", user, documentId, e);
      ctx.status(500)
          .json(new ErrorBody("Internal error", "Internal error: " + e.getMessage()));
    }
  }

  public record ErrorBody(String error, String message) {
    public ErrorBody(String error) {
      this(error, error);
    }
  }

  public record SuccessBody(
      boolean ok, String user, String folder, String document, String message) {}
}
