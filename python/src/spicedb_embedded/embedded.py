"""Embedded SpiceDB instance.

Uses in-memory transport: unary RPCs via FFI, streaming APIs (Watch, ReadRelationships, etc.)
via a streaming proxy. Use permissions(), schema(), and watch() to access the full SpiceDB API.
"""

import grpc
from authzed.api.v1 import (
    CheckBulkPermissionsRequest,
    CheckBulkPermissionsResponse,
    CheckPermissionRequest,
    CheckPermissionResponse,
    DeleteRelationshipsRequest,
    DeleteRelationshipsResponse,
    ExpandPermissionTreeRequest,
    ExpandPermissionTreeResponse,
    ReadRelationshipsRequest,
    ReadSchemaRequest,
    ReadSchemaResponse,
    Relationship,
    RelationshipUpdate,
    WriteRelationshipsRequest,
    WriteRelationshipsResponse,
    WriteSchemaRequest,
    WriteSchemaResponse,
)
from authzed.api.v1.permission_service_pb2_grpc import PermissionsServiceStub
from authzed.api.v1.watch_service_pb2_grpc import WatchServiceStub

from spicedb_embedded.errors import SpiceDBError
from spicedb_embedded.ffi import (
    spicedb_dispose,
    spicedb_permissions_check_bulk_permissions,
    spicedb_permissions_check_permission,
    spicedb_permissions_delete_relationships,
    spicedb_permissions_expand_permission_tree,
    spicedb_permissions_write_relationships,
    spicedb_schema_read_schema,
    spicedb_schema_write_schema,
    spicedb_start,
)
from spicedb_embedded.tcp_channel import get_target as get_tcp_target
from spicedb_embedded.unix_socket_channel import get_target as get_unix_socket_target


def _streaming_target(streaming_address: str, streaming_transport: str) -> str:
    """Return gRPC target for streaming_address using streaming_transport ('unix' or 'tcp')."""
    if streaming_transport == "unix":
        return get_unix_socket_target(streaming_address)
    return get_tcp_target(streaming_address)


class EmbeddedSpiceDB:
    """
    Embedded SpiceDB instance (in-memory only).

    Unary RPCs go through FFI; streaming (Watch, ReadRelationships, etc.) goes through
    the streaming proxy. Use permissions(), schema(), and watch() to access the API.
    """

    def __init__(
        self,
        schema: str,
        relationships: list[Relationship] | None = None,
        *,
        options: dict | None = None,
    ):
        """
        Create an embedded SpiceDB instance.

        Args:
            schema: The SpiceDB schema definition (ZED language).
            relationships: Initial relationships. Defaults to [].
            options: Optional config (datastore, datastore_uri, etc.). Instance is always in-memory.
        """
        data = spicedb_start(options)
        self._handle = data["handle"]
        self._streaming_address = data["streaming_address"]
        self._streaming_transport = data["streaming_transport"]
        target = _streaming_target(self._streaming_address, self._streaming_transport)
        self._channel = grpc.insecure_channel(target)

        try:
            self._bootstrap(schema, relationships or [])
        except Exception as e:
            self.close()
            raise SpiceDBError(f"Failed to bootstrap: {e}") from e

    def _bootstrap(self, schema: str, relationships: list[Relationship]) -> None:
        write_schema_req = WriteSchemaRequest(schema=schema)
        spicedb_schema_write_schema(self._handle, write_schema_req.SerializeToString())

        if relationships:
            updates = [
                RelationshipUpdate(
                    operation=RelationshipUpdate.Operation.OPERATION_TOUCH,
                    relationship=r,
                )
                for r in relationships
            ]
            write_rel_req = WriteRelationshipsRequest(updates=updates)
            spicedb_permissions_write_relationships(
                self._handle, write_rel_req.SerializeToString()
            )

    def permissions(self) -> "EmbeddedPermissionsStub":
        """Permissions service: unary via FFI, streaming (ReadRelationships, etc.) via proxy."""
        return EmbeddedPermissionsStub(self._handle, self._channel)

    def schema(self) -> "EmbeddedSchemaStub":
        """Schema service (ReadSchema, WriteSchema) via FFI."""
        return EmbeddedSchemaStub(self._handle)

    def watch(self) -> WatchServiceStub:
        """Watch service (streaming) via proxy."""
        return WatchServiceStub(self._channel)

    def channel(self) -> grpc.Channel:
        """Channel connected to the streaming proxy."""
        return self._channel

    def streaming_address(self) -> str:
        """Streaming proxy address (Unix path or host:port)."""
        return self._streaming_address

    def streaming_transport(self) -> str:
        """Streaming proxy transport: 'unix' or 'tcp'."""
        return self._streaming_transport

    def close(self) -> None:
        """Dispose the instance and close the channel."""
        spicedb_dispose(self._handle)
        self._channel.close()

    def __enter__(self) -> "EmbeddedSpiceDB":
        return self

    def __exit__(self, *args) -> None:
        self.close()


class EmbeddedPermissionsStub:
    """Permissions client: unary via FFI, ReadRelationships via streaming channel."""

    def __init__(self, handle: int, channel: grpc.Channel) -> None:
        self._handle = handle
        self._stub = PermissionsServiceStub(channel)

    def CheckPermission(
        self, request: CheckPermissionRequest
    ) -> CheckPermissionResponse:
        raw = spicedb_permissions_check_permission(
            self._handle, request.SerializeToString()
        )
        r = CheckPermissionResponse()
        r.ParseFromString(raw)
        return r

    def WriteRelationships(
        self, request: WriteRelationshipsRequest
    ) -> WriteRelationshipsResponse:
        raw = spicedb_permissions_write_relationships(
            self._handle, request.SerializeToString()
        )
        r = WriteRelationshipsResponse()
        r.ParseFromString(raw)
        return r

    def DeleteRelationships(
        self, request: DeleteRelationshipsRequest
    ) -> DeleteRelationshipsResponse:
        raw = spicedb_permissions_delete_relationships(
            self._handle, request.SerializeToString()
        )
        r = DeleteRelationshipsResponse()
        r.ParseFromString(raw)
        return r

    def CheckBulkPermissions(
        self, request: CheckBulkPermissionsRequest
    ) -> CheckBulkPermissionsResponse:
        raw = spicedb_permissions_check_bulk_permissions(
            self._handle, request.SerializeToString()
        )
        r = CheckBulkPermissionsResponse()
        r.ParseFromString(raw)
        return r

    def ExpandPermissionTree(
        self, request: ExpandPermissionTreeRequest
    ) -> ExpandPermissionTreeResponse:
        raw = spicedb_permissions_expand_permission_tree(
            self._handle, request.SerializeToString()
        )
        r = ExpandPermissionTreeResponse()
        r.ParseFromString(raw)
        return r

    def ReadRelationships(self, request: ReadRelationshipsRequest, **kwargs):
        """Streaming: uses the streaming proxy."""
        return self._stub.ReadRelationships(request, **kwargs)


class EmbeddedSchemaStub:
    """Schema client (ReadSchema, WriteSchema) via FFI."""

    def __init__(self, handle: int) -> None:
        self._handle = handle

    def ReadSchema(self, request: ReadSchemaRequest) -> ReadSchemaResponse:
        raw = spicedb_schema_read_schema(self._handle, request.SerializeToString())
        r = ReadSchemaResponse()
        r.ParseFromString(raw)
        return r

    def WriteSchema(self, request: WriteSchemaRequest) -> WriteSchemaResponse:
        raw = spicedb_schema_write_schema(self._handle, request.SerializeToString())
        r = WriteSchemaResponse()
        r.ParseFromString(raw)
        return r
