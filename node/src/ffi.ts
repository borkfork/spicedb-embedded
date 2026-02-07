/**
 * FFI bindings to the SpiceDB C-shared library (shared/c).
 */

import koffi from "koffi";
import { accessSync } from "fs";
import { platform } from "os";
import { resolve } from "path";

export interface SpiceDBStartResult {
  handle: number;
  socket_path: string;
}

function findLibrary(): string {
  const ext = platform() === "darwin" ? "dylib" : "so";
  const libName = `libspicedb.${ext}`;

  const explicit = process.env.SPICEDB_LIBRARY_PATH;
  if (explicit) {
    const candidate =
      explicit.endsWith(".so") || explicit.endsWith(".dylib")
        ? explicit
        : resolve(explicit, libName);
    try {
      accessSync(candidate);
      return candidate;
    } catch {
      return candidate;
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
  const HeapStr = koffi.disposable("HeapStr", "str", (ptr: unknown) =>
    spicedb_free(ptr)
  );

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
