using System.Diagnostics;
using Grpc.Core;
using Grpc.Core.Interceptors;

namespace Borkfork.SpiceDb.Embedded;

/// <summary>
///     The caller's W3C trace context, so SpiceDB's server spans join the caller's trace.
///     Reads <see cref="Activity.Current" />; an activity without a W3C id has no trace context.
/// </summary>
internal static class TraceContext
{
    internal const string TraceParent = "traceparent";
    internal const string TraceState = "tracestate";

    /// <summary>The current <c>traceparent</c> and <c>tracestate</c>, or nulls without a W3C activity.</summary>
    internal static (string? TraceParent, string? TraceState) Current()
    {
        var activity = Activity.Current;
        if (activity is not { IdFormat: ActivityIdFormat.W3C }) return (null, null);
        var traceState = string.IsNullOrEmpty(activity.TraceStateString) ? null : activity.TraceStateString;
        return (activity.Id, traceState);
    }
}

/// <summary>
///     Adds the current trace context to every call on the streaming proxy channel. A call that already
///     carries a <c>traceparent</c> keeps it. It runs before Grpc.Net.Client starts its own activity, so
///     SpiceDB's spans are children of the caller's activity, as they are for unary calls.
/// </summary>
internal sealed class TraceContextInterceptor : Interceptor
{
    internal static readonly TraceContextInterceptor Instance = new();

    public override TResponse BlockingUnaryCall<TRequest, TResponse>(TRequest request,
        ClientInterceptorContext<TRequest, TResponse> context,
        BlockingUnaryCallContinuation<TRequest, TResponse> continuation)
        => continuation(request, WithTraceContext(context));

    public override AsyncUnaryCall<TResponse> AsyncUnaryCall<TRequest, TResponse>(TRequest request,
        ClientInterceptorContext<TRequest, TResponse> context,
        AsyncUnaryCallContinuation<TRequest, TResponse> continuation)
        => continuation(request, WithTraceContext(context));

    public override AsyncServerStreamingCall<TResponse> AsyncServerStreamingCall<TRequest, TResponse>(
        TRequest request, ClientInterceptorContext<TRequest, TResponse> context,
        AsyncServerStreamingCallContinuation<TRequest, TResponse> continuation)
        => continuation(request, WithTraceContext(context));

    public override AsyncClientStreamingCall<TRequest, TResponse> AsyncClientStreamingCall<TRequest, TResponse>(
        ClientInterceptorContext<TRequest, TResponse> context,
        AsyncClientStreamingCallContinuation<TRequest, TResponse> continuation)
        => continuation(WithTraceContext(context));

    public override AsyncDuplexStreamingCall<TRequest, TResponse> AsyncDuplexStreamingCall<TRequest, TResponse>(
        ClientInterceptorContext<TRequest, TResponse> context,
        AsyncDuplexStreamingCallContinuation<TRequest, TResponse> continuation)
        => continuation(WithTraceContext(context));

    internal static ClientInterceptorContext<TRequest, TResponse> WithTraceContext<TRequest, TResponse>(
        ClientInterceptorContext<TRequest, TResponse> context)
        where TRequest : class
        where TResponse : class
    {
        var headers = context.Options.Headers;
        if (headers?.Get(TraceContext.TraceParent) != null) return context;
        var (traceParent, traceState) = TraceContext.Current();
        if (traceParent == null) return context;

        var withTrace = new Metadata();
        if (headers != null)
            foreach (var entry in headers)
                withTrace.Add(entry);
        withTrace.Add(TraceContext.TraceParent, traceParent);
        if (traceState != null) withTrace.Add(TraceContext.TraceState, traceState);
        return new ClientInterceptorContext<TRequest, TResponse>(context.Method, context.Host,
            context.Options.WithHeaders(withTrace));
    }
}
