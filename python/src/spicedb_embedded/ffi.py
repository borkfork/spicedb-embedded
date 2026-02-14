"""ctypes FFI bindings to the SpiceDB C-shared library.

Always uses in-memory transport; unary RPCs via FFI, streaming via proxy.
"""

import ctypes
import json
import os
import platform
import sys
from ctypes import POINTER, byref, c_char, c_char_p, c_int, c_ulonglong, c_void_p
from pathlib import Path

from spicedb_embedded.errors import SpiceDBError


def _platform_key() -> str | None:
    """Return natives dir name for this platform."""
    machine = platform.machine().lower()
    if machine in ("x86_64", "amd64"):
        arch = "x64"
    elif machine in ("arm64", "aarch64"):
        arch = "arm64"
    else:
        return None
    if sys.platform == "linux":
        return f"linux-{arch}"
    if sys.platform == "darwin":
        return f"darwin-{arch}"
    if sys.platform == "win32":
        return f"win32-{arch}"
    return None


def _find_library() -> str | None:
    """Find libspicedb path."""
    if sys.platform == "darwin":
        lib_name = "libspicedb.dylib"
    elif sys.platform == "win32":
        lib_name = "spicedb.dll"
    else:
        lib_name = "libspicedb.so"

    key = _platform_key()
    if key:
        bundled = Path(__file__).resolve().parent / "natives" / key / lib_name
        if bundled.exists():
            return str(bundled)

    explicit = os.environ.get("SPICEDB_LIBRARY_PATH")
    if explicit:
        return explicit

    return None


def _load_lib():
    """Load the SpiceDB C library."""
    path = _find_library()
    if path:
        return ctypes.CDLL(path)
    return ctypes.CDLL("spicedb")


_lib = None


def _get_lib():
    global _lib
    if _lib is None:
        _lib = _load_lib()
        _lib.spicedb_start.restype = POINTER(c_char)
        _lib.spicedb_start.argtypes = [c_char_p]
        _lib.spicedb_dispose.restype = POINTER(c_char)
        _lib.spicedb_dispose.argtypes = [c_ulonglong]
        _lib.spicedb_free.argtypes = [c_char_p]
        _lib.spicedb_free_bytes.argtypes = [c_void_p]

        # RPC signatures: handle, request_bytes, request_len, out_response_bytes, out_response_len, out_error
        _rpc_argtypes = [
            c_ulonglong,
            c_void_p,  # const uchar*
            c_int,
            POINTER(c_void_p),
            POINTER(c_int),
            POINTER(c_void_p),
        ]
        for name in (
            "spicedb_permissions_check_permission",
            "spicedb_schema_write_schema",
            "spicedb_permissions_write_relationships",
            "spicedb_permissions_delete_relationships",
            "spicedb_permissions_check_bulk_permissions",
            "spicedb_permissions_expand_permission_tree",
            "spicedb_schema_read_schema",
        ):
            getattr(_lib, name).restype = None
            getattr(_lib, name).argtypes = _rpc_argtypes
    return _lib


def _call_rpc(handle: int, request_bytes: bytes, rpc_name: str) -> bytes:
    """Call an RPC by name; return response bytes or raise."""
    lib = _get_lib()
    rpc = getattr(lib, rpc_name)
    out_resp = c_void_p()
    out_len = c_int()
    out_err = c_void_p()

    buf = ctypes.create_string_buffer(request_bytes)
    rpc(
        c_ulonglong(handle),
        buf,
        len(request_bytes),
        byref(out_resp),
        byref(out_len),
        byref(out_err),
    )

    if out_err.value:
        try:
            err_msg = ctypes.string_at(out_err.value).decode("utf-8", errors="replace")
            raise SpiceDBError(err_msg)
        finally:
            lib.spicedb_free(out_err.value)

    if out_len.value <= 0 or not out_resp.value:
        return b""
    try:
        return ctypes.string_at(out_resp.value, out_len.value)
    finally:
        lib.spicedb_free_bytes(out_resp.value)


def spicedb_start(options: dict | None = None) -> dict:
    """Start a new SpiceDB instance (always in-memory). Returns handle and streaming_address.

    Args:
        options: Optional config. grpc_transport is forced to "memory". Supported keys:
            datastore, datastore_uri, spanner_credentials_file, spanner_emulator_host,
            mysql_table_prefix, metrics_enabled.
    """
    opts = dict(options) if options else {}
    opts["grpc_transport"] = "memory"
    lib = _get_lib()
    options_json = json.dumps(opts).encode("utf-8") + b"\0"
    ptr = lib.spicedb_start(options_json)
    if not ptr:
        raise SpiceDBError("Null response from C library")

    try:
        raw = ctypes.string_at(ptr).decode("utf-8")
    finally:
        lib.spicedb_free(ptr)

    data = json.loads(raw)
    if not data.get("success", False):
        raise SpiceDBError(data.get("error", "Unknown error"))

    d = data["data"]
    if d.get("grpc_transport") != "memory" or "streaming_address" not in d:
        raise SpiceDBError(
            "Expected memory transport and streaming_address from C library"
        )
    return {"handle": d["handle"], "streaming_address": d["streaming_address"]}


def spicedb_dispose(handle: int) -> None:
    """Dispose a SpiceDB instance."""
    lib = _get_lib()
    ptr = lib.spicedb_dispose(c_ulonglong(handle))
    if ptr:
        try:
            raw = ctypes.string_at(ptr).decode("utf-8")
            doc = json.loads(raw)
            if not doc.get("success", True):
                raise SpiceDBError(doc.get("error", "Unknown error"))
        finally:
            lib.spicedb_free(ptr)


def spicedb_permissions_check_permission(handle: int, request_bytes: bytes) -> bytes:
    """FFI: CheckPermission. Returns marshalled CheckPermissionResponse."""
    return _call_rpc(handle, request_bytes, "spicedb_permissions_check_permission")


def spicedb_schema_write_schema(handle: int, request_bytes: bytes) -> bytes:
    """FFI: WriteSchema. Returns marshalled WriteSchemaResponse."""
    return _call_rpc(handle, request_bytes, "spicedb_schema_write_schema")


def spicedb_permissions_write_relationships(handle: int, request_bytes: bytes) -> bytes:
    """FFI: WriteRelationships. Returns marshalled WriteRelationshipsResponse."""
    return _call_rpc(handle, request_bytes, "spicedb_permissions_write_relationships")


def spicedb_permissions_delete_relationships(
    handle: int, request_bytes: bytes
) -> bytes:
    """FFI: DeleteRelationships. Returns marshalled DeleteRelationshipsResponse."""
    return _call_rpc(handle, request_bytes, "spicedb_permissions_delete_relationships")


def spicedb_permissions_check_bulk_permissions(
    handle: int, request_bytes: bytes
) -> bytes:
    """FFI: CheckBulkPermissions. Returns marshalled CheckBulkPermissionsResponse."""
    return _call_rpc(
        handle, request_bytes, "spicedb_permissions_check_bulk_permissions"
    )


def spicedb_permissions_expand_permission_tree(
    handle: int, request_bytes: bytes
) -> bytes:
    """FFI: ExpandPermissionTree. Returns marshalled ExpandPermissionTreeResponse."""
    return _call_rpc(
        handle, request_bytes, "spicedb_permissions_expand_permission_tree"
    )


def spicedb_schema_read_schema(handle: int, request_bytes: bytes) -> bytes:
    """FFI: ReadSchema. Returns marshalled ReadSchemaResponse."""
    return _call_rpc(handle, request_bytes, "spicedb_schema_read_schema")
