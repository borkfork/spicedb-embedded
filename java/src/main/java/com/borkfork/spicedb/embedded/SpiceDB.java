package com.borkfork.spicedb.embedded;

import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.Pointer;
import com.sun.jna.ptr.IntByReference;
import com.sun.jna.ptr.PointerByReference;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;

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
    String libPath = findLibraryPath();
    return Native.load(libPath != null ? libPath : "spicedb", SpiceDB.class);
  }

  static String findLibraryPath() {
    String explicit = System.getProperty("spicedb.library.path");
    String os = System.getProperty("os.name").toLowerCase();
    String arch = System.getProperty("os.arch").toLowerCase();
    String libName;
    if (os.contains("mac")) {
      libName = "libspicedb.dylib";
    } else if (os.contains("linux")) {
      libName = "libspicedb.so";
    } else if (os.contains("win")) {
      libName = "spicedb.dll";
    } else {
      libName = null;
    }
    if (explicit != null && libName != null) {
      Path dir = Paths.get(explicit).toAbsolutePath().normalize();
      Path lib = dir.resolve(libName);
      if (Files.isRegularFile(lib)) {
        return lib.toString();
      }
      return dir.resolve(libName).toString();
    }

    String platformKey = platformKey(os, arch);
    if (platformKey != null) {
      String resource = "natives/" + platformKey + "/" + libName;
      try (InputStream in = SpiceDB.class.getClassLoader().getResourceAsStream(resource)) {
        if (in != null) {
          Path tmp = Files.createTempFile("spicedb", libName);
          tmp.toFile().deleteOnExit();
          Files.copy(in, tmp, java.nio.file.StandardCopyOption.REPLACE_EXISTING);
          return tmp.toAbsolutePath().toString();
        }
      } catch (Exception ignored) {
        // fall through
      }
    }

    return null;
  }

  private static String platformKey(String os, String arch) {
    boolean x86_64 = arch.equals("amd64") || arch.equals("x86_64");
    boolean aarch_64 = arch.equals("aarch64") || arch.equals("arm64");
    if (os.contains("linux") && x86_64) return "linux-x86_64";
    if (os.contains("linux") && aarch_64) return "linux-aarch_64";
    if (os.contains("mac") && aarch_64) return "osx-aarch_64";
    if (os.contains("mac") && x86_64) return "osx-x86_64";
    if (os.contains("win") && x86_64) return "windows-x86_64";
    return null;
  }

  /**
   * Start a new SpiceDB instance (always in-memory).
   *
   * @param optionsJson JSON options. Pass null for defaults. grpc_transport is forced to "memory".
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
