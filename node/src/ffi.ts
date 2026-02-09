/**
 * FFI bindings to the SpiceDB C-shared library (shared/c).
 */

import koffi from "koffi";
import { accessSync } from "fs";
import { platform } from "os";
import { resolve } from "path";

export interface SpiceDBStartResult {
  handle: number;
  grpc_transport: "unix" | "tcp";
  address: string;
}

export interface SpiceDBStartOptions {
  /** Datastore: "memory" (default), "postgres", "cockroachdb", "spanner", "mysql" */
  datastore?: string;
  /** Connection string for remote datastores */
  datastore_uri?: string;
  /** gRPC transport: "unix" (default on Unix), "tcp" (default on Windows) */
  grpc_transport?: string;
  /** Path to Spanner service account JSON (Spanner only) */
  spanner_credentials_file?: string;
  /** Spanner emulator host (Spanner only) */
  spanner_emulator_host?: string;
  /** Prefix for all tables (MySQL only) */
  mysql_table_prefix?: string;
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
    const candidate = libExtensions.some((e) => explicit.endsWith(e))
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
  spicedb_start: (optionsJson: string | null) => string;
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
    spicedb_start: loaded.func(
      "HeapStr spicedb_start(const char* optionsJson)"
    ),
    spicedb_dispose: loaded.func(
      "HeapStr spicedb_dispose(unsigned long long handle)"
    ),
  };

  return lib;
}

export function spicedb_start(
  options?: SpiceDBStartOptions | null
): SpiceDBStartResult {
  const l = getLib();
  const optionsJson = options != null ? JSON.stringify(options) : null;
  const raw = l.spicedb_start(optionsJson);
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
