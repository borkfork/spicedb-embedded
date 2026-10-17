using System.Diagnostics;
using Authzed.Api.V1;
using Borkfork.SpiceDb.Embedded;
using Google.Protobuf;
using Grpc.Core;
using Grpc.Core.Interceptors;
using Xunit;

namespace SpiceDbEmbedded.Tests;

public class TraceContextTests
{
    private const string TraceId = "4bf92f3577b34da6a3ce929d0e0e4736";
    private const string SpanId = "00f067aa0ba902b7";
    private const string ParentTraceParent = "00-" + TraceId + "-" + SpanId + "-01";

    private static Activity StartActivity()
    {
        var activity = new Activity("test");
        activity.SetParentId(ParentTraceParent);
        activity.TraceStateString = "vendor=value";
        return activity.SetIdFormat(ActivityIdFormat.W3C).Start();
    }

    [Fact]
    public void CurrentIsEmptyWithoutAnActivity()
    {
        Assert.Null(Activity.Current);
        Assert.Equal((null, null), TraceContext.Current());
    }

    [Fact]
    public void CurrentCarriesTheCurrentActivity()
    {
        using var activity = StartActivity();
        var (traceParent, traceState) = TraceContext.Current();
        Assert.Equal($"00-{TraceId}-{activity.SpanId.ToHexString()}-01", traceParent);
        Assert.Equal("vendor=value", traceState);
    }

    [Fact]
    public void InterceptorAddsTheCurrentActivity()
    {
        using var activity = StartActivity();
        var headers = Intercept(new Metadata { { "a", "b" } });
        Assert.Equal("b", headers.GetValue("a"));
        Assert.Equal(activity.Id, headers.GetValue("traceparent"));
        Assert.Equal("vendor=value", headers.GetValue("tracestate"));
    }

    [Fact]
    public void InterceptorKeepsAnExistingTraceParent()
    {
        var existing = "00-" + new string('1', 32) + "-" + new string('2', 16) + "-01";
        using var activity = StartActivity();
        var headers = Intercept(new Metadata { { "traceparent", existing } });
        Assert.Equal(existing, headers.GetValue("traceparent"));
        Assert.Null(headers.Get("tracestate"));
    }

    [Fact]
    public async Task CallsSucceedUnderATraceContext()
    {
        using var spicedb = EmbeddedSpiceDb.Start(EmbeddedSpiceDbTests.TestSchema);
        using var activity = StartActivity();

        var schema = spicedb.Schema().ReadSchema(new ReadSchemaRequest());
        Assert.Contains("definition document", schema.SchemaText);

        var call = spicedb.Permissions().ReadRelationships(new ReadRelationshipsRequest
        {
            RelationshipFilter = new RelationshipFilter { ResourceType = "document" }
        });
        Assert.False(await call.ResponseStream.MoveNext(CancellationToken.None));
    }

    [Fact]
    public void EveryUnaryMethodNameIsKnown()
    {
        var calls = new Func<ulong, byte[], byte[]>[]
        {
            SpiceDbFfi.CheckPermission,
            SpiceDbFfi.WriteSchema,
            SpiceDbFfi.WriteRelationships,
            SpiceDbFfi.DeleteRelationships,
            SpiceDbFfi.CheckBulkPermissions,
            SpiceDbFfi.ExpandPermissionTree,
            SpiceDbFfi.ReadSchema
        };
        using var spicedb = EmbeddedSpiceDb.Start(EmbeddedSpiceDbTests.TestSchema);
        foreach (var call in calls)
            try
            {
                call(spicedb.Handle, []);
            }
            catch (SpiceDbException e)
            {
                // An empty request may fail validation, but its method must be one spicedb_rpc accepts.
                Assert.DoesNotContain("unsupported method", e.Message);
            }
    }

    [Fact]
    public void AnUnknownMethodIsAnError()
    {
        using var spicedb = EmbeddedSpiceDb.Start(EmbeddedSpiceDbTests.TestSchema);
        var e = Assert.Throws<SpiceDbException>(() =>
            SpiceDbFfi.Rpc(spicedb.Handle, "/authzed.api.v1.Nope/Nope", []));
        Assert.Equal("unsupported method: /authzed.api.v1.Nope/Nope", e.Message);
    }

    private static Metadata Intercept(Metadata headers)
    {
        var method = new Method<ReadSchemaRequest, ReadSchemaResponse>(MethodType.Unary, "s", "m",
            Marshallers.Create(r => r.ToByteArray(), ReadSchemaRequest.Parser.ParseFrom),
            Marshallers.Create(r => r.ToByteArray(), ReadSchemaResponse.Parser.ParseFrom));
        var context = new ClientInterceptorContext<ReadSchemaRequest, ReadSchemaResponse>(method, null,
            new CallOptions(headers));
        return TraceContextInterceptor.WithTraceContext(context).Options.Headers!;
    }
}
