import { AsyncLocalStorageContextManager } from "@opentelemetry/context-async-hooks";
import {
  context,
  createTraceState,
  trace,
  TraceFlags,
} from "@opentelemetry/api";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { v1, EmbeddedSpiceDB } from "../src/index.js";
import * as ffi from "../src/ffi.js";
import { currentTraceContext } from "../src/trace_context.js";

const TRACE_ID = "4bf92f3577b34da6a3ce929d0e0e4736";
const SPAN_ID = "00f067aa0ba902b7";

const TEST_SCHEMA = `
definition user {}

definition document {
    relation reader: user
    permission read = reader
}
`;

/** The native handle, private on EmbeddedSpiceDB. */
function handleOf(spicedb: EmbeddedSpiceDB): number {
  return (spicedb as unknown as { handle: number }).handle;
}

function withSpan<T>(fn: () => T): T {
  const span = trace.wrapSpanContext({
    traceId: TRACE_ID,
    spanId: SPAN_ID,
    traceFlags: TraceFlags.SAMPLED,
    traceState: createTraceState("vendor=value"),
  });
  return context.with(trace.setSpan(context.active(), span), fn);
}

describe("trace context", () => {
  const contextManager = new AsyncLocalStorageContextManager();

  beforeAll(() => {
    context.setGlobalContextManager(contextManager.enable());
  });

  afterAll(() => {
    context.disable();
  });

  it("is empty without a span", () => {
    expect(currentTraceContext()).toEqual({});
  });

  it("carries the active span", () => {
    withSpan(() => {
      expect(currentTraceContext()).toEqual({
        traceparent: `00-${TRACE_ID}-${SPAN_ID}-01`,
        tracestate: "vendor=value",
      });
    });
  });

  it(
    "lets unary and streaming calls succeed under a trace context",
    { timeout: 30000 },
    async () => {
      const spicedb = await EmbeddedSpiceDB.start(TEST_SCHEMA);
      try {
        await withSpan(async () => {
          const schema = await spicedb
            .schema()
            .promises.readSchema(v1.ReadSchemaRequest.create({}));
          expect(schema.schemaText).toContain("definition document");
          const rels = await spicedb.permissions().promises.readRelationships(
            v1.ReadRelationshipsRequest.create({
              relationshipFilter: v1.RelationshipFilter.create({
                resourceType: "document",
              }),
            })
          );
          expect(rels).toEqual([]);
        });
      } finally {
        spicedb.close();
      }
    }
  );

  it("sends every unary method by a name spicedb_rpc accepts", async () => {
    const calls = [
      ffi.spicedb_permissions_check_permission,
      ffi.spicedb_schema_write_schema,
      ffi.spicedb_permissions_write_relationships,
      ffi.spicedb_permissions_delete_relationships,
      ffi.spicedb_permissions_check_bulk_permissions,
      ffi.spicedb_permissions_expand_permission_tree,
      ffi.spicedb_schema_read_schema,
    ];
    const spicedb = await EmbeddedSpiceDB.start(TEST_SCHEMA);
    try {
      for (const call of calls) {
        try {
          call(handleOf(spicedb), new Uint8Array(0));
        } catch (e) {
          // An empty request may fail validation, but its method must be one spicedb_rpc accepts.
          expect(String(e)).not.toContain("unsupported method");
        }
      }
    } finally {
      spicedb.close();
    }
  });

  it("rejects an unknown method", async () => {
    const spicedb = await EmbeddedSpiceDB.start(TEST_SCHEMA);
    try {
      expect(() =>
        ffi.callRpc(
          "/authzed.api.v1.Nope/Nope",
          handleOf(spicedb),
          new Uint8Array(0)
        )
      ).toThrow("unsupported method: /authzed.api.v1.Nope/Nope");
    } finally {
      spicedb.close();
    }
  });
});
