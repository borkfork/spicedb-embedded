/**
 * FFI bindings to the SpiceDB C-shared library (shared/c).
 * Always uses in-memory transport; unary RPCs go through FFI, streaming via proxy.
 */

import koffi from "koffi";
import { createRequire } from "module";
import { accessSync } from "fs";
import { platform } from "os";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

/** Result of starting an embedded instance (memory transport). */
export interface SpiceDBStartResult {
  handle: number;
  /** Always "memory" when using this API. */
  grpc_transport: "memory";
  /** Address for streaming APIs (Watch, ReadRelationships, etc.). Unix path or host:port. */
  streaming_address: string;
}

/** Options for starting an embedded instance. Only datastore-related options are used. */
export interface SpiceDBStartOptions {
  /** Datastore: "memory" (default), "postgres", "cockroachdb", "spanner", "mysql" */
  datastore?: string;
  /** Connection string for remote datastores */
  datastore_uri?: string;
  /** Path to Spanner service account JSON (Spanner only) */
  spanner_credentials_file?: string;
  /** Spanner emulator host (Spanner only) */
  spanner_emulator_host?: string;
  /** Prefix for all tables (MySQL only) */
  mysql_table_prefix?: string;
  /** Enable datastore Prometheus metrics (default: false) */
  metrics_enabled?: boolean;
}

function findLibrary(): string {
  const currentPlatform = platform();

  const libName =
    currentPlatform === "win32"
      ? "spicedb.dll"
      : currentPlatform === "darwin"
        ? "libspicedb.dylib"
        : "libspicedb.so";

  const platformKey = `${currentPlatform}-${process.arch}`;
  const optionalPkgName = `spicedb-embedded-${platformKey}`;
  try {
    const req = createRequire(import.meta.url);
    const pkgRoot = dirname(req.resolve(`${optionalPkgName}/package.json`));
    const bundledPath = resolve(pkgRoot, "prebuilds", platformKey, libName);
    accessSync(bundledPath);
    return bundledPath;
  } catch {
    // optional dep not installed or no bundled lib for this platform
  }

  const prebuildsDir = resolve(
    dirname(fileURLToPath(import.meta.url)),
    "..",
    "prebuilds",
    platformKey
  );
  const legacyPath = resolve(prebuildsDir, libName);
  try {
    accessSync(legacyPath);
    return legacyPath;
  } catch {
    // no bundled lib for this platform
  }

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

  return libName;
}

interface SpiceDBLib {
  spicedb_start: (optionsJson: string | null) => string;
  spicedb_dispose: (handle: number) => string;
  spicedb_free: (ptr: unknown) => void;
  spicedb_free_bytes: (ptr: unknown) => void;
  spicedb_permissions_check_permission: (
    handle: number,
    requestBytes: Buffer,
    requestLen: number,
    outResponseBytes: [unknown],
    outResponseLen: [number],
    outError: [string | null]
  ) => void;
  spicedb_schema_write_schema: (
    handle: number,
    requestBytes: Buffer,
    requestLen: number,
    outResponseBytes: [unknown],
    outResponseLen: [number],
    outError: [string | null]
  ) => void;
  spicedb_permissions_write_relationships: (
    handle: number,
    requestBytes: Buffer,
    requestLen: number,
    outResponseBytes: [unknown],
    outResponseLen: [number],
    outError: [string | null]
  ) => void;
  spicedb_permissions_delete_relationships: (
    handle: number,
    requestBytes: Buffer,
    requestLen: number,
    outResponseBytes: [unknown],
    outResponseLen: [number],
    outError: [string | null]
  ) => void;
  spicedb_permissions_check_bulk_permissions: (
    handle: number,
    requestBytes: Buffer,
    requestLen: number,
    outResponseBytes: [unknown],
    outResponseLen: [number],
    outError: [string | null]
  ) => void;
  spicedb_permissions_expand_permission_tree: (
    handle: number,
    requestBytes: Buffer,
    requestLen: number,
    outResponseBytes: [unknown],
    outResponseLen: [number],
    outError: [string | null]
  ) => void;
  spicedb_schema_read_schema: (
    handle: number,
    requestBytes: Buffer,
    requestLen: number,
    outResponseBytes: [unknown],
    outResponseLen: [number],
    outError: [string | null]
  ) => void;
}

function copyOutAndFree(
  ptr: unknown,
  len: number,
  freeBytes: (p: unknown) => void
): Uint8Array {
  if (len <= 0) return new Uint8Array(0);
  if (ptr == null) return new Uint8Array(0);
  let copy: Uint8Array;
  if (ptr instanceof Buffer) {
    copy = new Uint8Array(ptr.subarray(0, len));
  } else if (ptr instanceof Uint8Array) {
    copy = ptr.slice(0, len);
  } else if (
    typeof ptr === "object" &&
    ptr !== null &&
    "buffer" in ptr &&
    (ptr as { buffer: ArrayBuffer }).buffer instanceof ArrayBuffer
  ) {
    const v = ptr as { buffer: ArrayBuffer; byteOffset?: number };
    copy = new Uint8Array(v.buffer, v.byteOffset ?? 0, len).slice();
  } else {
    try {
      const ab = koffi.view(ptr as Parameters<typeof koffi.view>[0], len);
      copy = new Uint8Array(ab).slice();
    } catch {
      copy = new Uint8Array(len);
    }
  }
  freeBytes(ptr);
  return copy;
}

let lib: SpiceDBLib | null = null;

function getLib(): SpiceDBLib {
  if (lib) return lib;

  const path = findLibrary();
  const loaded = koffi.load(path);

  const spicedb_free = loaded.func("void spicedb_free(char* ptr)");
  try {
    koffi.disposable("HeapStr", "str", (ptr: unknown) => spicedb_free(ptr));
  } catch {
    // Already registered (e.g. second getLib() call after first threw)
  }

  const spicedb_free_bytes = loaded.func("void spicedb_free_bytes(void* ptr)");

  const outUcharPtrPtr = koffi.out(koffi.pointer("uint8_t", 2));
  const outIntPtr = koffi.out(koffi.pointer("int"));
  const outCharPtrPtr = koffi.out(koffi.pointer("char", 2));
  const inBytes = koffi.pointer("uint8_t");

  lib = {
    spicedb_start: loaded.func(
      "HeapStr spicedb_start(const char* optionsJson)"
    ),
    spicedb_dispose: loaded.func(
      "HeapStr spicedb_dispose(unsigned long long handle)"
    ),
    spicedb_free: spicedb_free as (ptr: unknown) => void,
    spicedb_free_bytes,
    spicedb_permissions_check_permission: loaded.func(
      "spicedb_permissions_check_permission",
      "void",
      ["ulonglong", inBytes, "int", outUcharPtrPtr, outIntPtr, outCharPtrPtr]
    ),
    spicedb_schema_write_schema: loaded.func(
      "spicedb_schema_write_schema",
      "void",
      ["ulonglong", inBytes, "int", outUcharPtrPtr, outIntPtr, outCharPtrPtr]
    ),
    spicedb_permissions_write_relationships: loaded.func(
      "spicedb_permissions_write_relationships",
      "void",
      ["ulonglong", inBytes, "int", outUcharPtrPtr, outIntPtr, outCharPtrPtr]
    ),
    spicedb_permissions_delete_relationships: loaded.func(
      "spicedb_permissions_delete_relationships",
      "void",
      ["ulonglong", inBytes, "int", outUcharPtrPtr, outIntPtr, outCharPtrPtr]
    ),
    spicedb_permissions_check_bulk_permissions: loaded.func(
      "spicedb_permissions_check_bulk_permissions",
      "void",
      ["ulonglong", inBytes, "int", outUcharPtrPtr, outIntPtr, outCharPtrPtr]
    ),
    spicedb_permissions_expand_permission_tree: loaded.func(
      "spicedb_permissions_expand_permission_tree",
      "void",
      ["ulonglong", inBytes, "int", outUcharPtrPtr, outIntPtr, outCharPtrPtr]
    ),
    spicedb_schema_read_schema: loaded.func(
      "spicedb_schema_read_schema",
      "void",
      ["ulonglong", inBytes, "int", outUcharPtrPtr, outIntPtr, outCharPtrPtr]
    ),
  };

  return lib;
}

/** Start options forced to memory transport (no grpc_transport option exposed). */
function memoryStartOptions(
  options?: SpiceDBStartOptions | null
): Record<string, unknown> {
  const opts = options != null ? { ...options } : {};
  (opts as Record<string, unknown>).grpc_transport = "memory";
  return opts;
}

export function spicedb_start(
  options?: SpiceDBStartOptions | null
): SpiceDBStartResult {
  const l = getLib();
  const opts = memoryStartOptions(options);
  const optionsJson = JSON.stringify(opts);
  const raw = l.spicedb_start(optionsJson);
  if (!raw) throw new Error("Null response from C library");

  const data = JSON.parse(raw);

  if (!data.success) {
    throw new Error(data.error ?? "Unknown error");
  }

  const d = data.data;
  if (d.grpc_transport !== "memory" || !d.streaming_address) {
    throw new Error(
      "Expected memory transport and streaming_address from C library"
    );
  }

  return {
    handle: d.handle,
    grpc_transport: "memory",
    streaming_address: d.streaming_address,
  };
}

export function spicedb_dispose(handle: number): void {
  const l = getLib();
  const raw = l.spicedb_dispose(handle);
  const data = JSON.parse(raw);
  if (!data.success) {
    throw new Error(data.error ?? "Unknown error");
  }
}

function callRpc(
  fn: (
    handle: number,
    requestBytes: Buffer,
    requestLen: number,
    outResponseBytes: [unknown],
    outResponseLen: [number],
    outError: [string | null]
  ) => void,
  handle: number,
  requestBytes: Uint8Array
): Uint8Array {
  const l = getLib();
  const buf = Buffer.from(requestBytes);
  const outResp: [unknown] = [null];
  const outLen: [number] = [0];
  const outErr: [string | null] = [null];

  fn(handle, buf, buf.length, outResp, outLen, outErr);

  const errMsg = outErr[0];
  if (errMsg != null && errMsg !== "") {
    throw new Error(errMsg);
  }

  const ptr = outResp[0];
  const len = outLen[0];
  return copyOutAndFree(ptr, len, (p) => l.spicedb_free_bytes(p));
}

export function spicedb_permissions_check_permission(
  handle: number,
  requestBytes: Uint8Array
): Uint8Array {
  return callRpc(
    getLib().spicedb_permissions_check_permission,
    handle,
    requestBytes
  );
}

export function spicedb_schema_write_schema(
  handle: number,
  requestBytes: Uint8Array
): Uint8Array {
  return callRpc(getLib().spicedb_schema_write_schema, handle, requestBytes);
}

export function spicedb_permissions_write_relationships(
  handle: number,
  requestBytes: Uint8Array
): Uint8Array {
  return callRpc(
    getLib().spicedb_permissions_write_relationships,
    handle,
    requestBytes
  );
}

export function spicedb_permissions_delete_relationships(
  handle: number,
  requestBytes: Uint8Array
): Uint8Array {
  return callRpc(
    getLib().spicedb_permissions_delete_relationships,
    handle,
    requestBytes
  );
}

export function spicedb_permissions_check_bulk_permissions(
  handle: number,
  requestBytes: Uint8Array
): Uint8Array {
  return callRpc(
    getLib().spicedb_permissions_check_bulk_permissions,
    handle,
    requestBytes
  );
}

export function spicedb_permissions_expand_permission_tree(
  handle: number,
  requestBytes: Uint8Array
): Uint8Array {
  return callRpc(
    getLib().spicedb_permissions_expand_permission_tree,
    handle,
    requestBytes
  );
}

export function spicedb_schema_read_schema(
  handle: number,
  requestBytes: Uint8Array
): Uint8Array {
  return callRpc(getLib().spicedb_schema_read_schema, handle, requestBytes);
}
