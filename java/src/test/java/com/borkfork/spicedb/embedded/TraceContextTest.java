package com.borkfork.spicedb.embedded;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.authzed.api.v1.ReadSchemaRequest;
import io.opentelemetry.api.trace.Span;
import io.opentelemetry.api.trace.SpanContext;
import io.opentelemetry.api.trace.TraceFlags;
import io.opentelemetry.api.trace.TraceState;
import java.util.List;
import java.util.Map;
import java.util.function.BiFunction;
import org.junit.jupiter.api.Test;

class TraceContextTest {

  private static final String TRACE_ID = "4bf92f3577b34da6a3ce929d0e0e4736";
  private static final String SPAN_ID = "00f067aa0ba902b7";

  private static Span span() {
    return Span.wrap(
        SpanContext.create(
            TRACE_ID,
            SPAN_ID,
            TraceFlags.getSampled(),
            TraceState.builder().put("vendor", "value").build()));
  }

  @Test
  void currentIsEmptyWithoutASpan() {
    assertEquals(Map.of(), TraceContext.current());
  }

  @Test
  void currentCarriesTheCurrentSpan() {
    try (var scope = span().makeCurrent()) {
      Map<String, String> context = TraceContext.current();
      assertEquals("00-" + TRACE_ID + "-" + SPAN_ID + "-01", context.get(TraceContext.TRACEPARENT));
      assertEquals("vendor=value", context.get(TraceContext.TRACESTATE));
    }
  }

  @Test
  void unaryCallsSucceedUnderATraceContext() {
    try (var spicedb = EmbeddedSpiceDB.start(EmbeddedSpiceDBTest.TEST_SCHEMA, List.of());
        var scope = span().makeCurrent()) {
      var schema = spicedb.schema().readSchema(ReadSchemaRequest.getDefaultInstance());
      assertTrue(schema.getSchemaText().contains("definition document"));
    }
  }

  @Test
  void everyUnaryMethodNameIsKnown() {
    List<BiFunction<Long, byte[], byte[]>> calls =
        List.of(
            SpiceDBFfi::checkPermission,
            SpiceDBFfi::writeSchema,
            SpiceDBFfi::writeRelationships,
            SpiceDBFfi::deleteRelationships,
            SpiceDBFfi::checkBulkPermissions,
            SpiceDBFfi::expandPermissionTree,
            SpiceDBFfi::readSchema);
    try (var spicedb = EmbeddedSpiceDB.start(EmbeddedSpiceDBTest.TEST_SCHEMA, List.of())) {
      for (var call : calls) {
        try {
          call.apply(spicedb.handle(), new byte[0]);
        } catch (SpiceDBException e) {
          // An empty request may fail validation, but its method must be one spicedb_rpc accepts.
          assertFalse(e.getMessage().startsWith("unsupported method"), e.getMessage());
        }
      }
    }
  }

  @Test
  void anUnknownMethodIsAnError() {
    try (var spicedb = EmbeddedSpiceDB.start(EmbeddedSpiceDBTest.TEST_SCHEMA, List.of())) {
      var e =
          assertThrows(
              SpiceDBException.class,
              () -> SpiceDBFfi.callRpc(spicedb.handle(), "/authzed.api.v1.Nope/Nope", new byte[0]));
      assertEquals("unsupported method: /authzed.api.v1.Nope/Nope", e.getMessage());
    }
  }
}
