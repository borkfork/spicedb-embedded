package com.rendil.spicedb.embedded;

import com.authzed.api.v1.PermissionsServiceGrpc;
import com.authzed.api.v1.Relationship;
import com.authzed.api.v1.RelationshipUpdate;
import com.authzed.api.v1.SchemaServiceGrpc;
import com.authzed.api.v1.WatchServiceGrpc;
import com.authzed.api.v1.WriteRelationshipsRequest;
import com.authzed.api.v1.WriteSchemaRequest;
import com.google.gson.Gson;
import com.google.gson.JsonObject;
import com.sun.jna.Pointer;
import io.grpc.ManagedChannel;
import java.util.List;

/**
 * Embedded SpiceDB instance.
 *
 * <p>A thin wrapper that starts SpiceDB via the C-shared library (shared/c), connects over a Unix
 * socket, and bootstraps schema and relationships via gRPC.
 *
 * <p>Use {@link #permissions()}, {@link #schema()}, and {@link #watch()} to access the full SpiceDB
 * API.
 *
 * <p>Prerequisites: Build shared/c first ({@code mise run shared-c-build}).
 */
public final class EmbeddedSpiceDB implements AutoCloseable {

  private final long handle;
  private final ManagedChannel channel;

  private EmbeddedSpiceDB(long handle, ManagedChannel channel) {
    this.handle = handle;
    this.channel = channel;
  }

  /**
   * Create a new embedded SpiceDB instance with a schema and relationships.
   *
   * @param schema The SpiceDB schema definition (ZED language)
   * @param relationships Initial relationships (empty list allowed)
   * @return New EmbeddedSpiceDB instance
   */
  public static EmbeddedSpiceDB create(String schema, List<Relationship> relationships) {
    SpiceDB lib = SpiceDB.load();
    Pointer result = lib.spicedb_start();
    if (result == null) {
      throw new SpiceDBException("Null response from C library");
    }
    String json = result.getString(0);
    lib.spicedb_free(result);

    JsonObject parsed = new Gson().fromJson(json, JsonObject.class);
    if (parsed == null || !parsed.has("success") || !parsed.get("success").getAsBoolean()) {
      String err =
          parsed != null && parsed.has("error")
              ? parsed.getAsJsonPrimitive("error").getAsString()
              : "Unknown error";
      throw new SpiceDBException("Failed to start SpiceDB: " + err);
    }

    JsonObject data = parsed.getAsJsonObject("data");
    long handle = data.getAsJsonPrimitive("handle").getAsLong();
    String socketPath = data.getAsJsonPrimitive("socket_path").getAsString();

    ManagedChannel channel;
    try {
      channel = UnixSocketChannel.build(socketPath);
    } catch (Exception e) {
      // Dispose the instance on connection failure
      Pointer disposeResult = lib.spicedb_dispose(handle);
      if (disposeResult != null) {
        lib.spicedb_free(disposeResult);
      }
      throw new SpiceDBException("Failed to connect to SpiceDB: " + e.getMessage(), e);
    }

    EmbeddedSpiceDB db = new EmbeddedSpiceDB(handle, channel);

    // Bootstrap via gRPC
    var schemaStub = SchemaServiceGrpc.newBlockingStub(channel);
    schemaStub.writeSchema(WriteSchemaRequest.newBuilder().setSchema(schema).build());

    if (relationships != null && !relationships.isEmpty()) {
      var permStub = PermissionsServiceGrpc.newBlockingStub(channel);
      var updates =
          relationships.stream()
              .map(
                  r ->
                      RelationshipUpdate.newBuilder()
                          .setOperation(RelationshipUpdate.Operation.OPERATION_TOUCH)
                          .setRelationship(r)
                          .build())
              .toList();
      permStub.writeRelationships(
          WriteRelationshipsRequest.newBuilder().addAllUpdates(updates).build());
    }

    return db;
  }

  /**
   * @return Permissions service stub (CheckPermission, WriteRelationships, ReadRelationships, etc.)
   */
  public PermissionsServiceGrpc.PermissionsServiceBlockingStub permissions() {
    return PermissionsServiceGrpc.newBlockingStub(channel);
  }

  /**
   * @return Schema service stub (ReadSchema, WriteSchema, ReflectSchema, etc.)
   */
  public SchemaServiceGrpc.SchemaServiceBlockingStub schema() {
    return SchemaServiceGrpc.newBlockingStub(channel);
  }

  /**
   * @return Watch service stub (watch for relationship changes)
   */
  public WatchServiceGrpc.WatchServiceBlockingStub watch() {
    return WatchServiceGrpc.newBlockingStub(channel);
  }

  /**
   * @return The underlying gRPC channel for custom usage
   */
  public ManagedChannel channel() {
    return channel;
  }

  @Override
  public void close() {
    SpiceDB lib = SpiceDB.load();
    Pointer result = lib.spicedb_dispose(handle);
    if (result != null) {
      lib.spicedb_free(result);
    }
    channel.shutdown();
  }
}
