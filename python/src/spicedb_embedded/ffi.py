"""ctypes FFI bindings to the SpiceDB C-shared library."""

import ctypes
import json
import os
import platform
import sys
from ctypes import CDLL, POINTER, c_char, c_char_p, c_ulonglong
from pathlib import Path

from spicedb_embedded.errors import SpiceDBError


def _platform_key() -> str | None:
    """Return natives dir name for this platform (linux-x64, darwin-arm64, win32-x64)."""
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
    """Find libspicedb path. Prefers bundled wheel natives, then SPICEDB_LIBRARY_PATH, then shared/c."""
    if sys.platform == "darwin":
        lib_name = "libspicedb.dylib"
    elif sys.platform == "win32":
        lib_name = "spicedb.dll"
    else:
        lib_name = "libspicedb.so"

    # Bundled in wheel: package dir / natives / <platform> / <lib>
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
        return CDLL(path)
    return CDLL("spicedb")


_lib = None


def _get_lib():
    global _lib
    if _lib is None:
        _lib = _load_lib()
        # Use POINTER(c_char) to retain pointer for spicedb_free
        _lib.spicedb_start.restype = POINTER(c_char)
        _lib.spicedb_start.argtypes = [c_char_p]  # options_json, can be None
        _lib.spicedb_dispose.restype = POINTER(c_char)
        _lib.spicedb_free.argtypes = [c_char_p]
    return _lib


def spicedb_start(options: dict | None = None) -> dict:
    """Start a new SpiceDB instance. Returns dict with handle, grpc_transport, and address.

    Args:
        options: Optional config. Use None for defaults. Supported keys:
            - datastore: "memory" (default), "postgres", "cockroachdb", "spanner", "mysql"
            - datastore_uri: Connection string (required for remote datastores)
            - grpc_transport: "unix" (default on Unix), "tcp" (default on Windows)
            - spanner_credentials_file: Path to service account JSON (Spanner only)
            - spanner_emulator_host: e.g. "localhost:9010" (Spanner emulator)
            - mysql_table_prefix: Prefix for all tables (MySQL only, optional)
            - metrics_enabled: Enable datastore Prometheus metrics (default: False; disabled allows multiple instances in same process)
    """
    lib = _get_lib()
    options_json = json.dumps(options) if options else None
    ptr = lib.spicedb_start(options_json.encode("utf-8") if options_json else None)
    if not ptr:
        raise SpiceDBError("Null response from C library")

    try:
        raw = ctypes.string_at(ptr).decode("utf-8")
    finally:
        lib.spicedb_free(ptr)

    data = json.loads(raw)
    if not data.get("success", False):
        raise SpiceDBError(data.get("error", "Unknown error"))

    return data["data"]


def spicedb_dispose(handle: int) -> None:
    """Dispose a SpiceDB instance."""
    lib = _get_lib()
    ptr = lib.spicedb_dispose(c_ulonglong(handle))
    if ptr:
        lib.spicedb_free(ptr)
