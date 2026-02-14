package com.borkfork.spicedb.embedded;

import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;

/**
 * Holds cached native library path when loaded from JAR so the same library instance is used for
 * all FFI calls (avoids "invalid handle" from loading multiple copies).
 */
final class SpiceDBLibraryPath {

  private SpiceDBLibraryPath() {}

  /** Cached path when loaded from JAR; shared so we use one library instance. */
  private static volatile String cachedJarLibPath;

  static String getLibraryPath(Class<?> resourceClass) {
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
    }

    String platformKey = platformKey(os, arch);
    if (platformKey != null) {
      String cached = cachedJarLibPath;
      if (cached != null) {
        return cached;
      }
      String resource = "natives/" + platformKey + "/" + libName;
      synchronized (SpiceDBLibraryPath.class) {
        if (cachedJarLibPath != null) {
          return cachedJarLibPath;
        }
        try (InputStream in = resourceClass.getClassLoader().getResourceAsStream(resource)) {
          if (in != null) {
            Path tmp = Files.createTempFile("spicedb", libName);
            tmp.toFile().deleteOnExit();
            Files.copy(in, tmp, java.nio.file.StandardCopyOption.REPLACE_EXISTING);
            cachedJarLibPath = tmp.toAbsolutePath().toString();
            return cachedJarLibPath;
          }
        } catch (Exception ignored) {
          // fall through
        }
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
}
