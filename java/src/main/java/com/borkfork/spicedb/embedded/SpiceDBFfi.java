package com.borkfork.spicedb.embedded;

import com.sun.jna.Pointer;
import com.sun.jna.ptr.IntByReference;
import com.sun.jna.ptr.PointerByReference;
import java.util.Map;

/** Helper to call SpiceDB RPCs via JNA and return response bytes. */
final class SpiceDBFfi {

  private static final String PERMISSIONS = "/authzed.api.v1.PermissionsService/";
  private static final String SCHEMA = "/authzed.api.v1.SchemaService/";

  private SpiceDBFfi() {}

  /** Invokes a unary method by its full gRPC name under the current trace context. */
  static byte[] callRpc(long handle, String method, byte[] requestBytes) {
    SpiceDB lib = SpiceDB.load();
    PointerByReference outResp = new PointerByReference();
    IntByReference outLen = new IntByReference();
    PointerByReference outErr = new PointerByReference();
    Map<String, String> trace = TraceContext.current();

    lib.spicedb_rpc(
        handle,
        method,
        trace.get(TraceContext.TRACEPARENT),
        trace.get(TraceContext.TRACESTATE),
        requestBytes,
        requestBytes.length,
        outResp,
        outLen,
        outErr);

    Pointer errPtr = outErr.getValue();
    if (errPtr != null) {
      try {
        String msg = errPtr.getString(0, "UTF-8");
        throw new SpiceDBException(msg != null ? msg : "Unknown error");
      } finally {
        lib.spicedb_free(errPtr);
      }
    }

    Pointer respPtr = outResp.getValue();
    int len = outLen.getValue();
    if (len <= 0 || respPtr == null) {
      return new byte[0];
    }
    byte[] copy = respPtr.getByteArray(0, len);
    lib.spicedb_free_bytes(respPtr);
    return copy;
  }

  static byte[] checkPermission(long handle, byte[] requestBytes) {
    return callRpc(handle, PERMISSIONS + "CheckPermission", requestBytes);
  }

  static byte[] writeSchema(long handle, byte[] requestBytes) {
    return callRpc(handle, SCHEMA + "WriteSchema", requestBytes);
  }

  static byte[] writeRelationships(long handle, byte[] requestBytes) {
    return callRpc(handle, PERMISSIONS + "WriteRelationships", requestBytes);
  }

  static byte[] deleteRelationships(long handle, byte[] requestBytes) {
    return callRpc(handle, PERMISSIONS + "DeleteRelationships", requestBytes);
  }

  static byte[] checkBulkPermissions(long handle, byte[] requestBytes) {
    return callRpc(handle, PERMISSIONS + "CheckBulkPermissions", requestBytes);
  }

  static byte[] expandPermissionTree(long handle, byte[] requestBytes) {
    return callRpc(handle, PERMISSIONS + "ExpandPermissionTree", requestBytes);
  }

  static byte[] readSchema(long handle, byte[] requestBytes) {
    return callRpc(handle, SCHEMA + "ReadSchema", requestBytes);
  }
}
