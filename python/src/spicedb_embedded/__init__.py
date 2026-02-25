"""Embedded SpiceDB for Python — in-memory authorization server for tests and development."""

from spicedb_embedded.embedded import EmbeddedSpiceDB
from spicedb_embedded.errors import SpiceDBError
from spicedb_embedded.ffi import StartOptions

__all__ = ["EmbeddedSpiceDB", "SpiceDBError", "StartOptions"]
