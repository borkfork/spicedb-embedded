package com.borkfork.spicedb.embedded;

import io.grpc.CallOptions;
import io.grpc.Channel;
import io.grpc.ClientCall;
import io.grpc.ClientInterceptor;
import io.grpc.ForwardingClientCall;
import io.grpc.Metadata;
import io.grpc.MethodDescriptor;
import java.util.HashMap;
import java.util.Map;

/**
 * The caller's W3C trace context, so SpiceDB's server spans join the caller's trace.
 *
 * <p>Reads the current OpenTelemetry context when {@code io.opentelemetry:opentelemetry-api} is on
 * the classpath, and is empty otherwise. The OpenTelemetry references live in {@link Otel}, which
 * is loaded only after the classpath check.
 */
final class TraceContext {

  static final String TRACEPARENT = "traceparent";
  static final String TRACESTATE = "tracestate";

  private static final boolean OTEL_PRESENT = otelPresent();

  /** Adds the current trace context to every call on a streaming proxy channel. */
  static final ClientInterceptor INTERCEPTOR = new Interceptor();

  private TraceContext() {}

  /** The current {@code traceparent} and {@code tracestate}, or an empty map without one. */
  static Map<String, String> current() {
    return OTEL_PRESENT ? Otel.current() : Map.of();
  }

  private static boolean otelPresent() {
    try {
      Class.forName(
          "io.opentelemetry.api.trace.propagation.W3CTraceContextPropagator",
          false,
          TraceContext.class.getClassLoader());
      return true;
    } catch (ClassNotFoundException | LinkageError e) {
      return false;
    }
  }

  private static final class Otel {
    private Otel() {}

    static Map<String, String> current() {
      Map<String, String> carrier = new HashMap<>();
      io.opentelemetry.api.trace.propagation.W3CTraceContextPropagator.getInstance()
          .inject(io.opentelemetry.context.Context.current(), carrier, Map::put);
      return carrier;
    }
  }

  private static final class Interceptor implements ClientInterceptor {
    private static final Metadata.Key<String> TRACEPARENT_KEY =
        Metadata.Key.of(TRACEPARENT, Metadata.ASCII_STRING_MARSHALLER);
    private static final Metadata.Key<String> TRACESTATE_KEY =
        Metadata.Key.of(TRACESTATE, Metadata.ASCII_STRING_MARSHALLER);

    @Override
    public <ReqT, RespT> ClientCall<ReqT, RespT> interceptCall(
        MethodDescriptor<ReqT, RespT> method, CallOptions callOptions, Channel next) {
      return new ForwardingClientCall.SimpleForwardingClientCall<>(
          next.newCall(method, callOptions)) {
        @Override
        public void start(Listener<RespT> responseListener, Metadata headers) {
          Map<String, String> context = current();
          String traceparent = context.get(TRACEPARENT);
          if (traceparent != null && !headers.containsKey(TRACEPARENT_KEY)) {
            headers.put(TRACEPARENT_KEY, traceparent);
            String tracestate = context.get(TRACESTATE);
            if (tracestate != null) {
              headers.put(TRACESTATE_KEY, tracestate);
            }
          }
          super.start(responseListener, headers);
        }
      };
    }
  }
}
