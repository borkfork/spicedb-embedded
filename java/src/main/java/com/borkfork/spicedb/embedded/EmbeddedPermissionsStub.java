package com.borkfork.spicedb.embedded;

import com.authzed.api.v1.*;
import com.authzed.api.v1.PermissionsServiceGrpc;
import io.grpc.ManagedChannel;
import java.util.Iterator;

/** Permissions client: unary via FFI, ReadRelationships via streaming channel. */
public final class EmbeddedPermissionsStub {

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
