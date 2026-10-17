//! The caller's W3C trace context, so `SpiceDB`'s server spans join the caller's trace.
//!
//! With the `opentelemetry` feature, calls carry the current [`opentelemetry::Context`]. Without
//! it, each call starts its own trace.

pub use spicedb_embedded_sys::memory_transport::TraceContext;
use tonic::{Request, Status, metadata::MetadataValue};

const TRACEPARENT: &str = "traceparent";
const TRACESTATE: &str = "tracestate";

/// The current `traceparent` and `tracestate`, or an empty [`TraceContext`] without a valid span.
#[cfg(feature = "opentelemetry")]
#[must_use]
pub fn current() -> TraceContext {
    use opentelemetry::trace::TraceContextExt;

    let context = opentelemetry::Context::current();
    let span = context.span();
    let span_context = span.span_context();
    if !span_context.is_valid() {
        return TraceContext::default();
    }
    let tracestate = span_context.trace_state().header();
    TraceContext {
        traceparent: Some(format!(
            "00-{}-{}-{:02x}",
            span_context.trace_id(),
            span_context.span_id(),
            span_context.trace_flags().to_u8()
        )),
        tracestate: (!tracestate.is_empty()).then_some(tracestate),
    }
}

/// The current `traceparent` and `tracestate`; always empty without the `opentelemetry` feature.
#[cfg(not(feature = "opentelemetry"))]
#[must_use]
pub fn current() -> TraceContext {
    TraceContext::default()
}

/// A tonic interceptor that adds the current trace context to calls on the streaming proxy. A call
/// that already carries a `traceparent` keeps it.
///
/// ```ignore
/// let client = PermissionsServiceClient::with_interceptor(channel, spicedb_embedded::trace_context_interceptor);
/// ```
///
/// # Errors
///
/// Never; the signature is the one [`tonic::service::Interceptor`] takes.
pub fn interceptor(mut request: Request<()>) -> Result<Request<()>, Status> {
    if request.metadata().contains_key(TRACEPARENT) {
        return Ok(request);
    }
    let trace = current();
    let metadata = request.metadata_mut();
    for (key, value) in [
        (TRACEPARENT, trace.traceparent),
        (TRACESTATE, trace.tracestate),
    ] {
        if let Some(value) = value.and_then(|v| MetadataValue::try_from(v).ok()) {
            metadata.insert(key, value);
        }
    }
    Ok(request)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const TRACEPARENT_VALUE: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

    #[test]
    fn current_is_empty_without_a_span() {
        assert_eq!(current(), TraceContext::default());
    }

    #[test]
    fn interceptor_keeps_an_existing_traceparent() {
        let existing = format!("00-{}-{}-01", "1".repeat(32), "2".repeat(16));
        let mut request = Request::new(());
        request
            .metadata_mut()
            .insert(TRACEPARENT, existing.parse().unwrap());
        #[cfg(feature = "opentelemetry")]
        let _guard = otel::attach();
        let request = interceptor(request).unwrap();
        assert_eq!(
            request.metadata().get(TRACEPARENT).unwrap(),
            existing.as_str()
        );
        assert!(request.metadata().get(TRACESTATE).is_none());
    }

    #[cfg(feature = "opentelemetry")]
    pub(crate) mod otel {
        use opentelemetry::{
            Context, ContextGuard,
            trace::{SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState},
        };

        /// Makes a sampled span with `tracestate: vendor=value` current until the guard drops.
        pub(crate) fn attach() -> ContextGuard {
            let span_context = SpanContext::new(
                TraceId::from_hex("4bf92f3577b34da6a3ce929d0e0e4736").unwrap(),
                SpanId::from_hex("00f067aa0ba902b7").unwrap(),
                TraceFlags::SAMPLED,
                false,
                TraceState::from_key_value([("vendor", "value")]).unwrap(),
            );
            Context::new()
                .with_remote_span_context(span_context)
                .attach()
        }
    }

    #[cfg(feature = "opentelemetry")]
    #[test]
    fn current_carries_the_current_span() {
        let _guard = otel::attach();
        assert_eq!(
            current(),
            TraceContext {
                traceparent: Some(TRACEPARENT_VALUE.into()),
                tracestate: Some("vendor=value".into()),
            }
        );
    }

    #[cfg(feature = "opentelemetry")]
    #[test]
    fn interceptor_adds_the_current_span() {
        let _guard = otel::attach();
        let request = interceptor(Request::new(())).unwrap();
        assert_eq!(
            request.metadata().get(TRACEPARENT).unwrap(),
            TRACEPARENT_VALUE
        );
        assert_eq!(request.metadata().get(TRACESTATE).unwrap(), "vendor=value");
    }
}
