"""ctypes FFI bindings to the SpiceDB C-shared library."""

import ctypes
import json
import os
import sys
from ctypes import CDLL, POINTER, c_char, c_char_p, c_ulonglong
from pathlib import Path

from spicedb_embedded.errors import SpiceDBError


def _find_library() -> str | None:
    """Find libspicedb path. Searches: SPICEDB_LIBRARY_PATH, shared/c relative to cwd."""
    explicit = os.environ.get("SPICEDB_LIBRARY_PATH")
    if explicit:
        return explicit

    if sys.platform == "darwin":
        lib_name = "libspicedb.dylib"
    elif sys.platform == "win32":
        lib_name = "spicedb.dll"
    else:
        lib_name = "libspicedb.so"
    base = Path.cwd()
    for rel in ("shared/c", "../shared/c", "python/../shared/c"):
        path = (base / rel / lib_name).resolve()
        if path.exists():
            return str(path)
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
        options: Optional config: {"datastore": "memory", "grpc_transport": "unix"|"tcp", ...}.
            Use None for defaults.
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
