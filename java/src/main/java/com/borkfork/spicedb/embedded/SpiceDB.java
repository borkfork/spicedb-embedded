package com.borkfork.spicedb.embedded;

import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.Pointer;
import com.sun.jna.ptr.IntByReference;
import com.sun.jna.ptr.PointerByReference;

/**
 * JNA interface to the SpiceDB C-shared library (shared/c).
 *
 * <p>Always uses in-memory transport. Unary RPCs via FFI; streaming via proxy. Prefer bundled
 * natives from the JAR (natives/&lt;platform&gt;/). Otherwise uses spicedb.library.path or
 * shared/c.
 */
interface SpiceDB extends Library {

  /**
   * Load the native library. Prefers bundled JAR natives, then spicedb.library.path, then
   * "spicedb".
   */
  static SpiceDB load() {
    String libPath = SpiceDBLibraryPath.getLibraryPath(SpiceDB.class);
    return Native.load(libPath != null ? libPath : "spicedb", SpiceDB.class);
  }

  /**
   * Start a new SpiceDB instance (always in-memory).
   *
   * @param optionsJson JSON options. Pass null for defaults. Instance is always in-memory.
   * @return JSON: {"success": true, "data": {"handle": N, "streaming_address": "..."}}
   */
  Pointer spicedb_start(String optionsJson);

  /** Dispose a SpiceDB instance by handle. */
  Pointer spicedb_dispose(long handle);

  /** Free a string returned by spicedb_start or spicedb_dispose. */
  void spicedb_free(Pointer ptr);

  /** Free a byte buffer returned by the RPC FFI functions (out_response_bytes). */
  void spicedb_free_bytes(Pointer ptr);

  void spicedb_permissions_check_permission(
      long handle,
      byte[] requestBytes,
      int requestLen,
      PointerByReference outResponseBytes,
      IntByReference outResponseLen,
      PointerByReference outError);

  void spicedb_schema_write_schema(
      long handle,
      byte[] requestBytes,
      int requestLen,
      PointerByReference outResponseBytes,
      IntByReference outResponseLen,
      PointerByReference outError);

  void spicedb_permissions_write_relationships(
      long handle,
      byte[] requestBytes,
      int requestLen,
      PointerByReference outResponseBytes,
      IntByReference outResponseLen,
      PointerByReference outError);

  void spicedb_permissions_delete_relationships(
      long handle,
      byte[] requestBytes,
      int requestLen,
      PointerByReference outResponseBytes,
      IntByReference outResponseLen,
      PointerByReference outError);

  void spicedb_permissions_check_bulk_permissions(
      long handle,
      byte[] requestBytes,
      int requestLen,
      PointerByReference outResponseBytes,
      IntByReference outResponseLen,
      PointerByReference outError);

  void spicedb_permissions_expand_permission_tree(
      long handle,
      byte[] requestBytes,
      int requestLen,
      PointerByReference outResponseBytes,
      IntByReference outResponseLen,
      PointerByReference outError);

  void spicedb_schema_read_schema(
      long handle,
      byte[] requestBytes,
      int requestLen,
      PointerByReference outResponseBytes,
      IntByReference outResponseLen,
      PointerByReference outError);
}
