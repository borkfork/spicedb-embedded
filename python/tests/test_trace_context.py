"""Tests for trace context propagation."""

import pytest
from authzed.api.v1 import (
    ReadRelationshipsRequest,
    ReadSchemaRequest,
    RelationshipFilter,
)
from opentelemetry import trace
from opentelemetry.trace import NonRecordingSpan, SpanContext, TraceFlags, TraceState
from test_embedded import TEST_SCHEMA

from spicedb_embedded import EmbeddedSpiceDB, ffi, trace_context
from spicedb_embedded.errors import SpiceDBError
from spicedb_embedded.trace_context import TraceContextInterceptor, _CallDetails

TRACE_ID = "4bf92f3577b34da6a3ce929d0e0e4736"
SPAN_ID = "00f067aa0ba902b7"


def _span():
    return NonRecordingSpan(
        SpanContext(
            trace_id=int(TRACE_ID, 16),
            span_id=int(SPAN_ID, 16),
            is_remote=False,
            trace_flags=TraceFlags(TraceFlags.SAMPLED),
            trace_state=TraceState([("vendor", "value")]),
        )
    )


def _details(metadata=None):
    return _CallDetails("/m", None, metadata, None, None, None)


def test_current_is_empty_without_a_span():
    assert trace_context.current() == {}


def test_current_carries_the_current_span():
    with trace.use_span(_span()):
        assert trace_context.current() == {
            "traceparent": f"00-{TRACE_ID}-{SPAN_ID}-01",
            "tracestate": "vendor=value",
        }


def test_interceptor_adds_the_current_span():
    with trace.use_span(_span()):
        details = TraceContextInterceptor._details(_details([("a", "b")]))
    assert dict(details.metadata) == {
        "a": "b",
        "traceparent": f"00-{TRACE_ID}-{SPAN_ID}-01",
        "tracestate": "vendor=value",
    }


def test_interceptor_keeps_an_existing_traceparent():
    existing = [("traceparent", "00-" + "1" * 32 + "-" + "2" * 16 + "-01")]
    with trace.use_span(_span()):
        details = TraceContextInterceptor._details(_details(existing))
    assert list(details.metadata) == existing


def test_calls_succeed_under_a_trace_context():
    with EmbeddedSpiceDB.start(TEST_SCHEMA) as spicedb, trace.use_span(_span()):
        schema = spicedb.schema().ReadSchema(ReadSchemaRequest())
        assert "definition document" in schema.schema_text
        stream = spicedb.permissions().ReadRelationships(
            ReadRelationshipsRequest(
                relationship_filter=RelationshipFilter(resource_type="document")
            )
        )
        assert list(stream) == []


@pytest.mark.parametrize(
    "call",
    [
        ffi.spicedb_permissions_check_permission,
        ffi.spicedb_schema_write_schema,
        ffi.spicedb_permissions_write_relationships,
        ffi.spicedb_permissions_delete_relationships,
        ffi.spicedb_permissions_check_bulk_permissions,
        ffi.spicedb_permissions_expand_permission_tree,
        ffi.spicedb_schema_read_schema,
    ],
)
def test_every_unary_method_name_is_known(call):
    with EmbeddedSpiceDB.start(TEST_SCHEMA) as spicedb:
        try:
            call(spicedb._handle, b"")
        except SpiceDBError as e:
            # An empty request may fail validation, but its method must be one spicedb_rpc accepts.
            assert not str(e).startswith("unsupported method"), str(e)


def test_an_unknown_method_is_an_error():
    with (
        EmbeddedSpiceDB.start(TEST_SCHEMA) as spicedb,
        pytest.raises(
            SpiceDBError, match="^unsupported method: /authzed.api.v1.Nope/Nope$"
        ),
    ):
        ffi._call_rpc(spicedb._handle, b"", "/authzed.api.v1.Nope/Nope")
