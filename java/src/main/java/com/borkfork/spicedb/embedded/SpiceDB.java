package com.borkfork.spicedb.embedded;

import com.sun.jna.Library;
import com.sun.jna.Native;
import com.sun.jna.Pointer;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * JNA interface to the SpiceDB C-shared library (shared/c).
 *
 * <p>Prefer bundled natives from the JAR (natives/&lt;platform&gt;/). Otherwise uses
 * spicedb.library.path or shared/c relative to user.dir.
 */
interface SpiceDB extends Library {

  /**
   * Load the native library. Prefers bundled JAR natives, then spicedb.library.path, then shared/c.
   */
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
    String arch = System.getProperty("os.arch").toLowerCase();
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

    // Bundled natives in JAR: natives/<platform>/<lib>
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
        // fall through to filesystem search
      }
    }

    return null;
  }

  /**
   * Maps os.name + os.arch to resource dir names (matches os-maven-plugin: linux-x86_64,
   * osx-aarch_64, windows-x86_64).
   */
  private static String platformKey(String os, String arch) {
    boolean x86_64 = arch.equals("amd64") || arch.equals("x86_64");
    boolean aarch_64 = arch.equals("aarch64") || arch.equals("arm64");
    if (os.contains("linux") && x86_64) return "linux-x86_64";
    if (os.contains("mac") && aarch_64) return "osx-aarch_64";
    if (os.contains("mac") && x86_64) return "osx-x86_64";
    if (os.contains("win") && x86_64) return "windows-x86_64";
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
