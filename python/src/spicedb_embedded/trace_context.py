"""The caller's W3C trace context, so SpiceDB's server spans join the caller's trace.

Reads the current OpenTelemetry context when ``opentelemetry-api`` is installed, and is empty
otherwise.
"""

import collections

import grpc

TRACEPARENT = "traceparent"
TRACESTATE = "tracestate"

try:
    from opentelemetry.trace.propagation.tracecontext import (
        TraceContextTextMapPropagator,
    )

    _propagator = TraceContextTextMapPropagator()
except ImportError:  # opentelemetry-api is optional
    _propagator = None


def current() -> dict[str, str]:
    """The current ``traceparent`` and ``tracestate``, or an empty dict without one."""
    carrier: dict[str, str] = {}
    if _propagator is not None:
        _propagator.inject(carrier)
    return carrier


class _CallDetails(
    collections.namedtuple(
        "_CallDetails",
        (
            "method",
            "timeout",
            "metadata",
            "credentials",
            "wait_for_ready",
            "compression",
        ),
    ),
    grpc.ClientCallDetails,
):
    pass


class TraceContextInterceptor(
    grpc.UnaryUnaryClientInterceptor,
    grpc.UnaryStreamClientInterceptor,
    grpc.StreamUnaryClientInterceptor,
    grpc.StreamStreamClientInterceptor,
):
    """Adds the current trace context to every call on a streaming proxy channel.

    A call that already carries a ``traceparent`` keeps it.
    """

    @staticmethod
    def _details(details: grpc.ClientCallDetails) -> grpc.ClientCallDetails:
        metadata = list(details.metadata or ())
        if any(key == TRACEPARENT for key, _ in metadata):
            return details
        context = current()
        if TRACEPARENT not in context:
            return details
        metadata.extend(context.items())
        return _CallDetails(
            details.method,
            details.timeout,
            metadata,
            details.credentials,
            getattr(details, "wait_for_ready", None),
            getattr(details, "compression", None),
        )

    def intercept_unary_unary(self, continuation, client_call_details, request):
        return continuation(self._details(client_call_details), request)

    def intercept_unary_stream(self, continuation, client_call_details, request):
        return continuation(self._details(client_call_details), request)

    def intercept_stream_unary(
        self, continuation, client_call_details, request_iterator
    ):
        return continuation(self._details(client_call_details), request_iterator)

    def intercept_stream_stream(
        self, continuation, client_call_details, request_iterator
    ):
        return continuation(self._details(client_call_details), request_iterator)
