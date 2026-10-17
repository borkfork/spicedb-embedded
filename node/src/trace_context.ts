/**
 * The caller's W3C trace context, so SpiceDB's server spans join the caller's trace.
 *
 * Reads the active OpenTelemetry context when `@opentelemetry/api` is installed, and is empty
 * otherwise.
 */

import * as grpc from "@grpc/grpc-js";
import { createRequire } from "module";

export const TRACEPARENT = "traceparent";
export const TRACESTATE = "tracestate";

type OtelApi = typeof import("@opentelemetry/api");

function loadOtel(): OtelApi | null {
  try {
    return createRequire(import.meta.url)("@opentelemetry/api") as OtelApi;
  } catch {
    return null; // @opentelemetry/api is an optional peer dependency
  }
}

const otel = loadOtel();

/** The active `traceparent` and `tracestate`, or an empty object without one. */
export function currentTraceContext(): Record<string, string> {
  if (otel == null) return {};
  const spanContext = otel.trace.getSpanContext(otel.context.active());
  if (spanContext == null || !otel.isSpanContextValid(spanContext)) return {};
  const flags = (spanContext.traceFlags & 0xff).toString(16).padStart(2, "0");
  const carrier: Record<string, string> = {
    [TRACEPARENT]: `00-${spanContext.traceId}-${spanContext.spanId}-${flags}`,
  };
  const tracestate = spanContext.traceState?.serialize();
  if (tracestate) carrier[TRACESTATE] = tracestate;
  return carrier;
}

/**
 * Adds the active trace context to every call on the streaming proxy client. A call that already
 * carries a `traceparent` keeps it.
 */
export const traceContextInterceptor: grpc.Interceptor = (options, nextCall) =>
  new grpc.InterceptingCall(nextCall(options), {
    start(metadata, listener, next) {
      if (metadata.get(TRACEPARENT).length === 0) {
        for (const [key, value] of Object.entries(currentTraceContext())) {
          metadata.set(key, value);
        }
      }
      next(metadata, listener);
    },
  });
