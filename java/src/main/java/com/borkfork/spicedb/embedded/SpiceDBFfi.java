package com.borkfork.spicedb.embedded;

import com.sun.jna.Pointer;
import com.sun.jna.ptr.IntByReference;
import com.sun.jna.ptr.PointerByReference;

/** Helper to call SpiceDB RPCs via JNA and return response bytes. */
final class SpiceDBFfi {

  private SpiceDBFfi() {}

  static byte[] callRpc(long handle, byte[] requestBytes, RpcCall rpc) {
    SpiceDB lib = SpiceDB.load();
    PointerByReference outResp = new PointerByReference();
    IntByReference outLen = new IntByReference();
    PointerByReference outErr = new PointerByReference();

    rpc.call(lib, handle, requestBytes, requestBytes.length, outResp, outLen, outErr);

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

  interface RpcCall {
    void call(
        SpiceDB lib,
        long handle,
        byte[] requestBytes,
        int requestLen,
        PointerByReference outResp,
        IntByReference outLen,
        PointerByReference outErr);
  }

  static byte[] checkPermission(long handle, byte[] requestBytes) {
    return callRpc(
        handle,
        requestBytes,
        (lib, h, req, len, outResp, outLen, outErr) ->
            lib.spicedb_permissions_check_permission(h, req, len, outResp, outLen, outErr));
  }

  static byte[] writeSchema(long handle, byte[] requestBytes) {
    return callRpc(
        handle,
        requestBytes,
        (lib, h, req, len, outResp, outLen, outErr) ->
            lib.spicedb_schema_write_schema(h, req, len, outResp, outLen, outErr));
  }

  static byte[] writeRelationships(long handle, byte[] requestBytes) {
    return callRpc(
        handle,
        requestBytes,
        (lib, h, req, len, outResp, outLen, outErr) ->
            lib.spicedb_permissions_write_relationships(h, req, len, outResp, outLen, outErr));
  }

  static byte[] deleteRelationships(long handle, byte[] requestBytes) {
    return callRpc(
        handle,
        requestBytes,
        (lib, h, req, len, outResp, outLen, outErr) ->
            lib.spicedb_permissions_delete_relationships(h, req, len, outResp, outLen, outErr));
  }

  static byte[] checkBulkPermissions(long handle, byte[] requestBytes) {
    return callRpc(
        handle,
        requestBytes,
        (lib, h, req, len, outResp, outLen, outErr) ->
            lib.spicedb_permissions_check_bulk_permissions(h, req, len, outResp, outLen, outErr));
  }

  static byte[] expandPermissionTree(long handle, byte[] requestBytes) {
    return callRpc(
        handle,
        requestBytes,
        (lib, h, req, len, outResp, outLen, outErr) ->
            lib.spicedb_permissions_expand_permission_tree(h, req, len, outResp, outLen, outErr));
  }

  static byte[] readSchema(long handle, byte[] requestBytes) {
    return callRpc(
        handle,
        requestBytes,
        (lib, h, req, len, outResp, outLen, outErr) ->
            lib.spicedb_schema_read_schema(h, req, len, outResp, outLen, outErr));
  }
}
