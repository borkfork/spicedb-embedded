package com.borkfork.spicedb.embedded;

import com.authzed.api.v1.*;
import com.authzed.api.v1.PermissionsServiceGrpc;
import com.authzed.api.v1.WatchServiceGrpc;
import com.google.gson.Gson;
import com.google.gson.JsonObject;
import com.sun.jna.Pointer;
import io.grpc.ManagedChannel;
import java.util.Iterator;
import java.util.List;

/**
 * Embedded SpiceDB instance (in-memory only).
 *
 * <p>Unary RPCs go through FFI; streaming (Watch, ReadRelationships, etc.) goes through the
 * streaming proxy. Use {@link #permissions()}, {@link #schema()}, and {@link #watch()} to access
 * the full SpiceDB API.
 *
 * <p>Prerequisites: Build shared/c first ({@code mise run shared-c-build}).
 */
public final class EmbeddedSpiceDB implements AutoCloseable {

  private final long handle;
  private final ManagedChannel channel;
  private final String streamingAddress;

  private EmbeddedSpiceDB(long handle, ManagedChannel channel, String streamingAddress) {
    this.handle = handle;
    this.channel = channel;
    this.streamingAddress = streamingAddress;
  }

  /**
   * Create a new embedded SpiceDB instance with a schema and relationships.
   *
   * @param schema The SpiceDB schema definition (ZED language)
   * @param relationships Initial relationships (empty list allowed)
   * @return New EmbeddedSpiceDB instance
   */
  public static EmbeddedSpiceDB create(String schema, List<Relationship> relationships) {
    return create(schema, relationships, null);
  }

  /**
   * Create a new embedded SpiceDB instance with a schema, relationships, and options.
   *
   * @param schema The SpiceDB schema definition (ZED language)
   * @param relationships Initial relationships (empty list allowed)
   * @param options Optional datastore options. Pass null for defaults.
   * @return New EmbeddedSpiceDB instance
   */
  public static EmbeddedSpiceDB create(
      String schema, List<Relationship> relationships, StartOptions options) {
    SpiceDB lib = SpiceDB.load();
    StartOptions opts = options != null ? options : new StartOptions();
    String optionsJson = new Gson().toJson(opts);
    Pointer result = lib.spicedb_start(optionsJson);
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
    String streamingAddr = data.getAsJsonPrimitive("streaming_address").getAsString();
    String streamingTransport = data.getAsJsonPrimitive("streaming_transport").getAsString();

    ManagedChannel ch;
    try {
      ch =
          "unix".equalsIgnoreCase(streamingTransport)
              ? UnixSocketChannel.build(streamingAddr)
              : TcpChannel.build(streamingAddr);
    } catch (Exception e) {
      Pointer disposeResult = lib.spicedb_dispose(handle);
      if (disposeResult != null) {
        lib.spicedb_free(disposeResult);
      }
      throw new SpiceDBException("Failed to connect to streaming proxy: " + e.getMessage(), e);
    }

    EmbeddedSpiceDB db = new EmbeddedSpiceDB(handle, ch, streamingAddr);

    // Bootstrap via FFI
    WriteSchemaRequest schemaReq = WriteSchemaRequest.newBuilder().setSchema(schema).build();
    SpiceDBFfi.writeSchema(handle, schemaReq.toByteArray());

    if (relationships != null && !relationships.isEmpty()) {
      var updates =
          relationships.stream()
              .map(
                  r ->
                      RelationshipUpdate.newBuilder()
                          .setOperation(RelationshipUpdate.Operation.OPERATION_TOUCH)
                          .setRelationship(r)
                          .build())
              .toList();
      WriteRelationshipsRequest relReq =
          WriteRelationshipsRequest.newBuilder().addAllUpdates(updates).build();
      SpiceDBFfi.writeRelationships(handle, relReq.toByteArray());
    }

    return db;
  }

  /** Permissions service: unary via FFI, ReadRelationships via streaming proxy. */
  public EmbeddedPermissionsStub permissions() {
    return new EmbeddedPermissionsStub(handle, channel);
  }

  /** Schema service (ReadSchema, WriteSchema) via FFI. */
  public EmbeddedSchemaStub schema() {
    return new EmbeddedSchemaStub(handle);
  }

  /** Watch service (streaming) via proxy. */
  public WatchServiceGrpc.WatchServiceBlockingStub watch() {
    return WatchServiceGrpc.newBlockingStub(channel);
  }

  /** The underlying gRPC channel (streaming proxy). */
  public ManagedChannel channel() {
    return channel;
  }

  /** Streaming proxy address (Unix path or host:port). */
  public String streamingAddress() {
    return streamingAddress;
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

/** Permissions client: unary via FFI, ReadRelationships via streaming channel. */
final class EmbeddedPermissionsStub {

  private final long handle;
  private final ManagedChannel channel;

  EmbeddedPermissionsStub(long handle, ManagedChannel channel) {
    this.handle = handle;
    this.channel = channel;
  }

  public CheckPermissionResponse checkPermission(CheckPermissionRequest request) {
    byte[] raw = SpiceDBFfi.checkPermission(handle, request.toByteArray());
    if (raw.length == 0) {
      return CheckPermissionResponse.getDefaultInstance();
    }
    try {
      return CheckPermissionResponse.parseFrom(raw);
    } catch (Exception e) {
      throw new SpiceDBException("Failed to parse CheckPermissionResponse", e);
    }
  }

  public WriteRelationshipsResponse writeRelationships(WriteRelationshipsRequest request) {
    byte[] raw = SpiceDBFfi.writeRelationships(handle, request.toByteArray());
    if (raw.length == 0) {
      return WriteRelationshipsResponse.getDefaultInstance();
    }
    try {
      return WriteRelationshipsResponse.parseFrom(raw);
    } catch (Exception e) {
      throw new SpiceDBException("Failed to parse WriteRelationshipsResponse", e);
    }
  }

  public DeleteRelationshipsResponse deleteRelationships(DeleteRelationshipsRequest request) {
    byte[] raw = SpiceDBFfi.deleteRelationships(handle, request.toByteArray());
    if (raw.length == 0) {
      return DeleteRelationshipsResponse.getDefaultInstance();
    }
    try {
      return DeleteRelationshipsResponse.parseFrom(raw);
    } catch (Exception e) {
      throw new SpiceDBException("Failed to parse DeleteRelationshipsResponse", e);
    }
  }

  public CheckBulkPermissionsResponse checkBulkPermissions(CheckBulkPermissionsRequest request) {
    byte[] raw = SpiceDBFfi.checkBulkPermissions(handle, request.toByteArray());
    if (raw.length == 0) {
      return CheckBulkPermissionsResponse.getDefaultInstance();
    }
    try {
      return CheckBulkPermissionsResponse.parseFrom(raw);
    } catch (Exception e) {
      throw new SpiceDBException("Failed to parse CheckBulkPermissionsResponse", e);
    }
  }

  public ExpandPermissionTreeResponse expandPermissionTree(ExpandPermissionTreeRequest request) {
    byte[] raw = SpiceDBFfi.expandPermissionTree(handle, request.toByteArray());
    if (raw.length == 0) {
      return ExpandPermissionTreeResponse.getDefaultInstance();
    }
    try {
      return ExpandPermissionTreeResponse.parseFrom(raw);
    } catch (Exception e) {
      throw new SpiceDBException("Failed to parse ExpandPermissionTreeResponse", e);
    }
  }

  /** Streaming: uses the streaming proxy. */
  public Iterator<ReadRelationshipsResponse> readRelationships(ReadRelationshipsRequest request) {
    return PermissionsServiceGrpc.newBlockingStub(channel).readRelationships(request);
  }
}

/** Schema client (ReadSchema, WriteSchema) via FFI. */
final class EmbeddedSchemaStub {

  private final long handle;

  EmbeddedSchemaStub(long handle) {
    this.handle = handle;
  }

  public ReadSchemaResponse readSchema(ReadSchemaRequest request) {
    byte[] raw = SpiceDBFfi.readSchema(handle, request.toByteArray());
    if (raw.length == 0) {
      return ReadSchemaResponse.getDefaultInstance();
    }
    try {
      return ReadSchemaResponse.parseFrom(raw);
    } catch (Exception e) {
      throw new SpiceDBException("Failed to parse ReadSchemaResponse", e);
    }
  }

  public WriteSchemaResponse writeSchema(WriteSchemaRequest request) {
    byte[] raw = SpiceDBFfi.writeSchema(handle, request.toByteArray());
    if (raw.length == 0) {
      return WriteSchemaResponse.getDefaultInstance();
    }
    try {
      return WriteSchemaResponse.parseFrom(raw);
    } catch (Exception e) {
      throw new SpiceDBException("Failed to parse WriteSchemaResponse", e);
    }
  }
}
