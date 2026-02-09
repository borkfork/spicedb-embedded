package com.borkfork.spicedb.embedded;

import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.Pointer;

/**
 * JNA interface to the SpiceDB C-shared library (shared/c).
 *
 * <p>Build shared/c first: {@code mise run shared-c-build} or {@code cd shared/c && CGO_ENABLED=1
 * go build -buildmode=c-shared -o libspicedb.dylib .}
 */
interface SpiceDB extends Library {

  /** Load the native library. Searches: java.library.path, spicedb.library.path, ../shared/c. */
  static SpiceDB load() {
    String libPath = findLibraryPath();
    return Native.load(libPath != null ? libPath : "spicedb", SpiceDB.class);
  }

  static String findLibraryPath() {
    String explicit = System.getProperty("spicedb.library.path");
    if (explicit != null) {
      return explicit;
    }
    String os = System.getProperty("os.name").toLowerCase();
    String libName;
    if (os.contains("mac")) {
      libName = "libspicedb.dylib";
    } else if (os.contains("linux")) {
      libName = "libspicedb.so";
    } else if (os.contains("win")) {
      libName = "spicedb.dll";
    } else {
      return null;
    }
    String base = System.getProperty("user.dir");
    // Try shared/c relative to user.dir (when running from project root or java/)
    for (String rel : new String[] {"shared/c", "../shared/c", "java/../shared/c"}) {
      java.nio.file.Path path =
          java.nio.file.Path.of(base).resolve(rel).resolve(libName).normalize();
      if (java.nio.file.Files.exists(path)) {
        return path.toAbsolutePath().toString();
      }
    }
    return null;
  }

  /**
   * Start a new SpiceDB instance.
   *
   * @param optionsJson Optional JSON options. Pass null for defaults.
   * @return JSON: {"success": true, "data": {"handle": N, "grpc_transport": "unix"|"tcp",
   *     "address": "..."}}
   */
  Pointer spicedb_start(String optionsJson);

  /** Dispose a SpiceDB instance by handle. */
  Pointer spicedb_dispose(long handle);

  /** Free a string returned by spicedb_start or spicedb_dispose. */
  void spicedb_free(Pointer ptr);
}
