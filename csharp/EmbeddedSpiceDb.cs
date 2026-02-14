using Authzed.Api.V1;
using Google.Protobuf;
using Grpc.Core;
using Grpc.Net.Client;

namespace Borkfork.SpiceDb.Embedded;

/// <summary>
///     Embedded SpiceDB instance. Uses in-memory transport: unary RPCs via FFI,
///     streaming APIs (Watch, ReadRelationships, etc.) via a streaming proxy.
///     Use Permissions(), Schema(), and Watch() to access the full SpiceDB API.
/// </summary>
// ReSharper disable UnusedMember.Global, UnusedParameter.Global -- Public API; options match gRPC CallOptions
public sealed class EmbeddedSpiceDb : IDisposable
{
    private readonly ulong _handle;
    private bool _disposed;

    private EmbeddedSpiceDb(ulong handle, GrpcChannel streamingChannel, string streamingAddress)
    {
        _handle = handle;
        StreamingChannel = streamingChannel;
        StreamingAddress = streamingAddress;
    }

    /// <summary>
    ///     Channel connected to the streaming proxy (for Watch, ReadRelationships, etc.).
    /// </summary>
    // ReSharper disable once MemberCanBePrivate.Global -- Public API for advanced streaming usage
    public GrpcChannel StreamingChannel { get; }

    /// <summary>
    ///     Streaming proxy address (Unix path or host:port).
    /// </summary>
    // ReSharper disable once UnusedAutoPropertyAccessor.Global -- Public API
    public string StreamingAddress { get; }

    /// <inheritdoc />
    public void Dispose()
    {
        if (_disposed) return;
        SpiceDbFfi.Dispose(_handle);
        StreamingChannel.Dispose();
        _disposed = true;
    }

    /// <summary>
    ///     Create a new embedded SpiceDB instance (in-memory only).
    /// </summary>
    /// <param name="schema">The SpiceDB schema definition (ZED language)</param>
    /// <param name="relationships">Initial relationships (empty list or null allowed)</param>
    /// <param name="options">Optional datastore options. Pass null for defaults.</param>
    /// <returns>New EmbeddedSpiceDb instance</returns>
    public static EmbeddedSpiceDb Create(
        string schema,
        IReadOnlyList<Relationship>? relationships = null,
        StartOptions? options = null)
    {
        var start = SpiceDbFfi.Start(options);

        var httpHandler = CreateHandlerForStreaming(start.StreamingAddress, start.StreamingTransport);
        var httpClient = new HttpClient(httpHandler) { BaseAddress = new Uri("http://localhost") };
        var channelOptions = new GrpcChannelOptions
        {
            HttpClient = httpClient,
            Credentials = ChannelCredentials.Insecure
        };

        GrpcChannel channel;
        try
        {
            channel = GrpcChannel.ForAddress("http://localhost", channelOptions);
        }
        catch (Exception ex)
        {
            try { SpiceDbFfi.Dispose(start.Handle); } catch { /* best effort */ }
            throw new SpiceDbException("Failed to connect to streaming proxy: " + ex.Message, ex);
        }

        var db = new EmbeddedSpiceDb(start.Handle, channel, start.StreamingAddress);

        try
        {
            db.Bootstrap(schema, relationships ?? Array.Empty<Relationship>());
        }
        catch (Exception ex)
        {
            db.Dispose();
            throw new SpiceDbException("Failed to bootstrap: " + ex.Message, ex);
        }

        return db;
    }

    private static SocketsHttpHandler CreateHandlerForStreaming(string streamingAddress, string streamingTransport)
    {
        return string.Equals(streamingTransport, "unix", StringComparison.OrdinalIgnoreCase)
            ? UnixSocketChannel.CreateHandler(streamingAddress)
            : TcpChannel.CreateHandler(streamingAddress);
    }

    private void Bootstrap(string schema, IReadOnlyList<Relationship> relationships)
    {
        var writeSchemaReq = new WriteSchemaRequest { Schema = schema };
        SpiceDbFfi.WriteSchema(_handle, writeSchemaReq.ToByteArray());

        if (relationships.Count > 0)
        {
            var updates = relationships.Select(r => new RelationshipUpdate
            {
                Operation = RelationshipUpdate.Types.Operation.Touch,
                Relationship = r
            }).ToList();
            var writeRelReq = new WriteRelationshipsRequest { Updates = { updates } };
            _ = SpiceDbFfi.WriteRelationships(_handle, writeRelReq.ToByteArray());
        }
    }

    /// <summary>
    ///     Permissions service: unary RPCs via FFI, streaming (ReadRelationships, etc.) via proxy.
    /// </summary>
    public EmbeddedPermissionsClient Permissions() => new(_handle, StreamingChannel);

    /// <summary>
    ///     Schema service (ReadSchema, WriteSchema) via FFI.
    /// </summary>
    public EmbeddedSchemaClient Schema() => new(_handle, StreamingChannel);

    /// <summary>
    ///     Watch service (streaming) via proxy.
    /// </summary>
    public WatchService.WatchServiceClient Watch() => new(StreamingChannel);
}

/// <summary>
///     Permissions client: unary calls go through FFI, streaming through the proxy.
/// </summary>
// ReSharper disable UnusedParameter.Global -- options match gRPC CallOptions
public sealed class EmbeddedPermissionsClient
{
    private readonly ulong _handle;
    private readonly PermissionsService.PermissionsServiceClient _streamingClient;

    internal EmbeddedPermissionsClient(ulong handle, GrpcChannel streamingChannel)
    {
        _handle = handle;
        _streamingClient = new PermissionsService.PermissionsServiceClient(streamingChannel);
    }

    public CheckPermissionResponse CheckPermission(CheckPermissionRequest request, CallOptions? options = null)
    {
        var bytes = SpiceDbFfi.CheckPermission(_handle, request.ToByteArray());
        return bytes.Length == 0 ? new CheckPermissionResponse() : CheckPermissionResponse.Parser.ParseFrom(bytes);
    }

    public WriteRelationshipsResponse WriteRelationships(WriteRelationshipsRequest request, CallOptions? options = null)
    {
        var bytes = SpiceDbFfi.WriteRelationships(_handle, request.ToByteArray());
        return bytes.Length == 0 ? new WriteRelationshipsResponse() : WriteRelationshipsResponse.Parser.ParseFrom(bytes);
    }

    public DeleteRelationshipsResponse DeleteRelationships(DeleteRelationshipsRequest request, CallOptions? options = null)
    {
        var bytes = SpiceDbFfi.DeleteRelationships(_handle, request.ToByteArray());
        return bytes.Length == 0 ? new DeleteRelationshipsResponse() : DeleteRelationshipsResponse.Parser.ParseFrom(bytes);
    }

    public CheckBulkPermissionsResponse CheckBulkPermissions(CheckBulkPermissionsRequest request, CallOptions? options = null)
    {
        var bytes = SpiceDbFfi.CheckBulkPermissions(_handle, request.ToByteArray());
        return bytes.Length == 0 ? new CheckBulkPermissionsResponse() : CheckBulkPermissionsResponse.Parser.ParseFrom(bytes);
    }

    public ExpandPermissionTreeResponse ExpandPermissionTree(ExpandPermissionTreeRequest request, CallOptions? options = null)
    {
        var bytes = SpiceDbFfi.ExpandPermissionTree(_handle, request.ToByteArray());
        return bytes.Length == 0 ? new ExpandPermissionTreeResponse() : ExpandPermissionTreeResponse.Parser.ParseFrom(bytes);
    }

    public AsyncServerStreamingCall<ReadRelationshipsResponse> ReadRelationships(ReadRelationshipsRequest request, CallOptions? options = null)
        => _streamingClient.ReadRelationships(request, options ?? default);
}

/// <summary>
///     Schema client (ReadSchema, WriteSchema) via FFI.
/// </summary>
// ReSharper disable UnusedParameter.Global -- options match gRPC CallOptions
public sealed class EmbeddedSchemaClient
{
    private readonly ulong _handle;

    // ReSharper disable once UnusedParameter.Local -- Same signature as EmbeddedPermissionsClient for consistency
    internal EmbeddedSchemaClient(ulong handle, GrpcChannel _)
    {
        _handle = handle;
    }

    public ReadSchemaResponse ReadSchema(ReadSchemaRequest request, CallOptions? options = null)
    {
        var bytes = SpiceDbFfi.ReadSchema(_handle, request.ToByteArray());
        return bytes.Length == 0 ? new ReadSchemaResponse() : ReadSchemaResponse.Parser.ParseFrom(bytes);
    }

    public WriteSchemaResponse WriteSchema(WriteSchemaRequest request, CallOptions? options = null)
    {
        var bytes = SpiceDbFfi.WriteSchema(_handle, request.ToByteArray());
        return bytes.Length == 0 ? new WriteSchemaResponse() : WriteSchemaResponse.Parser.ParseFrom(bytes);
    }
}
