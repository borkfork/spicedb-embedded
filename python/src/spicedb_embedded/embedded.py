"""Embedded SpiceDB instance."""

import grpc
from authzed.api.v1 import (
    Consistency,
    ObjectReference,
    Relationship,
    RelationshipUpdate,
    SubjectReference,
    WriteRelationshipsRequest,
    WriteSchemaRequest,
)
from authzed.api.v1.permission_service_pb2_grpc import PermissionsServiceStub
from authzed.api.v1.schema_service_pb2_grpc import SchemaServiceStub
from authzed.api.v1.watch_service_pb2_grpc import WatchServiceStub

from spicedb_embedded.errors import SpiceDBError
from spicedb_embedded.ffi import spicedb_dispose, spicedb_start
from spicedb_embedded.tcp_channel import get_target as get_tcp_target
from spicedb_embedded.unix_socket_channel import get_target as get_unix_socket_target


class EmbeddedSpiceDB:
    """
    Embedded SpiceDB instance.

    A thin wrapper that starts SpiceDB via the C-shared library, connects over a Unix
    socket, and bootstraps schema and relationships via gRPC.

    Use permissions(), schema(), and watch() to access the full SpiceDB API.
    """

    def __init__(self, schema: str, relationships: list[Relationship] | None = None):
        data = spicedb_start(None)
        self._handle = data["handle"]
        grpc_transport = data["grpc_transport"]
        address = data["address"]

        target = (
            get_unix_socket_target(address)
            if grpc_transport == "unix"
            else get_tcp_target(address)
        )
        self._channel = grpc.insecure_channel(target)

        try:
            self._bootstrap(schema, relationships or [])
        except Exception as e:
            self.close()
            raise SpiceDBError(f"Failed to bootstrap: {e}") from e

    def _bootstrap(self, schema: str, relationships: list[Relationship]) -> None:
        schema_stub = SchemaServiceStub(self._channel)
        schema_stub.WriteSchema(WriteSchemaRequest(schema=schema))

        if relationships:
            perm_stub = PermissionsServiceStub(self._channel)
            updates = [
                RelationshipUpdate(
                    operation=RelationshipUpdate.Operation.OPERATION_TOUCH,
                    relationship=r,
                )
                for r in relationships
            ]
            perm_stub.WriteRelationships(WriteRelationshipsRequest(updates=updates))

    def permissions(self) -> PermissionsServiceStub:
        """Permissions service stub (CheckPermission, WriteRelationships, etc.)."""
        return PermissionsServiceStub(self._channel)

    def schema(self) -> SchemaServiceStub:
        """Schema service stub (ReadSchema, WriteSchema, ReflectSchema, etc.)."""
        return SchemaServiceStub(self._channel)

    def watch(self) -> WatchServiceStub:
        """Watch service stub for relationship changes."""
        return WatchServiceStub(self._channel)

    def channel(self) -> grpc.Channel:
        """Underlying gRPC channel."""
        return self._channel

    def close(self) -> None:
        """Dispose the instance and close the channel."""
        spicedb_dispose(self._handle)
        self._channel.close()

    def __enter__(self) -> "EmbeddedSpiceDB":
        return self

    def __exit__(self, *args) -> None:
        self.close()
