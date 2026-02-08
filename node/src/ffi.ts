/**
 * FFI bindings to the SpiceDB C-shared library (shared/c).
 */

import koffi from "koffi";
import { accessSync } from "fs";
import { platform } from "os";
import { resolve } from "path";

export interface SpiceDBStartResult {
  handle: number;
  transport: "unix" | "tcp";
  address: string;
}

function findLibrary(): string {
  const currentPlatform = platform();

  const libName =
    currentPlatform === "win32"
      ? "spicedb.dll"
      : currentPlatform === "darwin"
        ? "libspicedb.dylib"
        : "libspicedb.so";

  const explicit = process.env.SPICEDB_LIBRARY_PATH;
  if (explicit) {
    const libExtensions = [".so", ".dylib", ".dll"];
    const candidate =
      libExtensions.some((e) => explicit.endsWith(e))
        ? explicit
        : resolve(explicit, libName);
    try {
      accessSync(candidate);
      return candidate;
    } catch (err) {
      const message =
        err instanceof Error && err.message ? err.message : String(err);
      throw new Error(
        `Unable to access SpiceDB library at '${candidate}' specified by SPICEDB_LIBRARY_PATH: ${message}`
      );
    }
  }

  const searchPaths = [
    "shared/c",
    resolve(process.cwd(), "shared/c"),
    resolve(process.cwd(), "../shared/c"),
    resolve(process.cwd(), "node/../shared/c"),
  ];

  for (const base of searchPaths) {
    const candidate = resolve(base, libName);
    try {
      accessSync(candidate);
      return candidate;
    } catch {
      // continue
    }
  }

  return libName;
}

interface SpiceDBLib {
  spicedb_start: () => string;
  spicedb_dispose: (handle: number) => string;
}

let lib: SpiceDBLib | null = null;

function getLib(): SpiceDBLib {
  if (lib) return lib;

  const path = findLibrary();
  const loaded = koffi.load(path);

  const spicedb_free = loaded.func("void spicedb_free(char* ptr)");
  koffi.disposable("HeapStr", "str", (ptr: unknown) => spicedb_free(ptr));

  lib = {
    spicedb_start: loaded.func("HeapStr spicedb_start()"),
    spicedb_dispose: loaded.func(
      "HeapStr spicedb_dispose(unsigned long long handle)"
    ),
  };

  return lib;
}

export function spicedb_start(): SpiceDBStartResult {
  const l = getLib();
  const raw = l.spicedb_start();
  if (!raw) throw new Error("Null response from C library");

  const data = JSON.parse(raw);

  if (!data.success) {
    throw new Error(data.error ?? "Unknown error");
  }

  return data.data;
}

export function spicedb_dispose(handle: number): void {
  const l = getLib();
  const raw = l.spicedb_dispose(handle);
  const data = JSON.parse(raw);
  if (!data.success) {
    throw new Error(data.error ?? "Unknown error");
  }
}
