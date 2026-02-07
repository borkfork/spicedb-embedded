"""Tests for EmbeddedSpiceDB."""

from authzed.api.v1 import (
    CheckPermissionRequest,
    CheckPermissionResponse,
    Consistency,
    ObjectReference,
    Relationship,
    RelationshipUpdate,
    SubjectReference,
    WriteRelationshipsRequest,
)

from spicedb_embedded import EmbeddedSpiceDB


def rel(resource: str, relation: str, subject: str) -> Relationship:
    """Create a Relationship from resource_type:id, relation, subject_type:id."""
    res_type, res_id = resource.split(":")
    sub_type, sub_id = subject.split(":")
    return Relationship(
        resource=ObjectReference(object_type=res_type, object_id=res_id),
        relation=relation,
        subject=SubjectReference(
            object=ObjectReference(object_type=sub_type, object_id=sub_id)
        ),
    )


TEST_SCHEMA = """
definition user {}

definition document {
    relation reader: user
    relation writer: user

    permission read = reader + writer
    permission write = writer
}
"""


def test_check_permission():
    """Test CheckPermission with initial relationships."""
    relationships = [
        rel("document:readme", "reader", "user:alice"),
        rel("document:readme", "writer", "user:bob"),
    ]

    with EmbeddedSpiceDB(TEST_SCHEMA, relationships) as spicedb:
        consistency = Consistency(fully_consistent=True)
        check_req = CheckPermissionRequest(
            consistency=consistency,
            resource=ObjectReference(object_type="document", object_id="readme"),
            permission="read",
            subject=SubjectReference(
                object=ObjectReference(object_type="user", object_id="alice")
            ),
        )

        response = spicedb.permissions().CheckPermission(check_req)
        assert (
            response.permissionship
            == CheckPermissionResponse.PERMISSIONSHIP_HAS_PERMISSION
        )

        # Alice has read, not write
        write_req = CheckPermissionRequest(
            consistency=consistency,
            resource=ObjectReference(object_type="document", object_id="readme"),
            permission="write",
            subject=SubjectReference(
                object=ObjectReference(object_type="user", object_id="alice")
            ),
        )
        write_resp = spicedb.permissions().CheckPermission(write_req)
        assert (
            write_resp.permissionship
            == CheckPermissionResponse.PERMISSIONSHIP_NO_PERMISSION
        )

        # Bob has read and write
        bob_read_req = CheckPermissionRequest(
            consistency=consistency,
            resource=ObjectReference(object_type="document", object_id="readme"),
            permission="read",
            subject=SubjectReference(
                object=ObjectReference(object_type="user", object_id="bob")
            ),
        )
        bob_read_resp = spicedb.permissions().CheckPermission(bob_read_req)
        assert (
            bob_read_resp.permissionship
            == CheckPermissionResponse.PERMISSIONSHIP_HAS_PERMISSION
        )


def test_add_relationship():
    """Test adding a relationship after creation."""
    with EmbeddedSpiceDB(TEST_SCHEMA, []) as spicedb:
        spicedb.permissions().WriteRelationships(
            WriteRelationshipsRequest(
                updates=[
                    RelationshipUpdate(
                        operation=RelationshipUpdate.Operation.OPERATION_TOUCH,
                        relationship=rel("document:test", "reader", "user:alice"),
                    )
                ]
            )
        )

        check_req = CheckPermissionRequest(
            consistency=Consistency(fully_consistent=True),
            resource=ObjectReference(object_type="document", object_id="test"),
            permission="read",
            subject=SubjectReference(
                object=ObjectReference(object_type="user", object_id="alice")
            ),
        )
        response = spicedb.permissions().CheckPermission(check_req)
        assert (
            response.permissionship
            == CheckPermissionResponse.PERMISSIONSHIP_HAS_PERMISSION
        )
