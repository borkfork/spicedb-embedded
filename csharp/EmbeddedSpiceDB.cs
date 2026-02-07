using System.Net.Http;
using System.Net.Sockets;
using System.Net;
using Authzed.Api.V1;
using Grpc.Core;
using Grpc.Net.Client;

namespace Rendil.Spicedb.Embedded;

/// <summary>
/// Embedded SpiceDB instance.
/// A thin wrapper that starts SpiceDB via the C-shared library, connects over a Unix
/// socket, and bootstraps schema and relationships via gRPC.
/// Use Permissions(), Schema(), and Watch() to access the full SpiceDB API.
/// </summary>
public sealed class EmbeddedSpiceDB : IDisposable
{
    private readonly ulong _handle;
    private readonly GrpcChannel _channel;
    private bool _disposed;

    private EmbeddedSpiceDB(ulong handle, GrpcChannel channel)
    {
        _handle = handle;
        _channel = channel;
    }

    /// <summary>
    /// Create a new embedded SpiceDB instance with a schema and relationships.
    /// </summary>
    /// <param name="schema">The SpiceDB schema definition (ZED language)</param>
    /// <param name="relationships">Initial relationships (empty list or null allowed)</param>
    /// <returns>New EmbeddedSpiceDB instance</returns>
    public static EmbeddedSpiceDB Create(string schema, IReadOnlyList<Relationship>? relationships = null)
    {
        var (handle, socketPath) = SpiceDBFFI.Start();

        var httpHandler = new SocketsHttpHandler
        {
            ConnectCallback = async (ctx, ct) =>
            {
                var socket = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.IP);
                var endpoint = new UnixDomainSocketEndPoint(socketPath);
                await socket.ConnectAsync(endpoint, ct);
                return new NetworkStream(socket, ownsSocket: true);
            },
        };

        var httpClient = new HttpClient(httpHandler) { BaseAddress = new Uri("http://localhost") };

        var options = new GrpcChannelOptions
        {
            HttpClient = httpClient,
            Credentials = ChannelCredentials.Insecure,
        };

        GrpcChannel channel;
        try
        {
            channel = GrpcChannel.ForAddress("http://localhost", options);
        }
        catch (Exception ex)
        {
            try { SpiceDBFFI.Dispose(handle); } catch { /* best effort */ }
            throw new SpiceDBException("Failed to connect to SpiceDB: " + ex.Message, ex);
        }

        var db = new EmbeddedSpiceDB(handle, channel);

        try
        {
            db.Bootstrap(schema, relationships ?? Array.Empty<Relationship>());
        }
        catch (Exception ex)
        {
            db.Dispose();
            throw new SpiceDBException("Failed to bootstrap: " + ex.Message, ex);
        }

        return db;
    }

    private void Bootstrap(string schema, IReadOnlyList<Relationship> relationships)
    {
        var schemaClient = new SchemaService.SchemaServiceClient(_channel);
        schemaClient.WriteSchema(new WriteSchemaRequest { Schema = schema });

        if (relationships.Count > 0)
        {
            var permClient = new PermissionsService.PermissionsServiceClient(_channel);
            var updates = relationships.Select(r => new RelationshipUpdate
            {
                Operation = RelationshipUpdate.Types.Operation.Touch,
                Relationship = r,
            }).ToList();
            permClient.WriteRelationships(new WriteRelationshipsRequest { Updates = { updates } });
        }
    }

    /// <summary>
    /// Permissions service client (CheckPermission, WriteRelationships, ReadRelationships, etc.).
    /// </summary>
    public PermissionsService.PermissionsServiceClient Permissions() =>
        new PermissionsService.PermissionsServiceClient(_channel);

    /// <summary>
    /// Schema service client (ReadSchema, WriteSchema, ReflectSchema, etc.).
    /// </summary>
    public SchemaService.SchemaServiceClient Schema() =>
        new SchemaService.SchemaServiceClient(_channel);

    /// <summary>
    /// Watch service client for relationship changes.
    /// </summary>
    public WatchService.WatchServiceClient Watch() =>
        new WatchService.WatchServiceClient(_channel);

    /// <summary>
    /// Underlying gRPC channel for custom usage.
    /// </summary>
    public GrpcChannel Channel => _channel;

    /// <inheritdoc />
    public void Dispose()
    {
        if (_disposed) return;
        SpiceDBFFI.Dispose(_handle);
        _channel.Dispose();
        _disposed = true;
        GC.SuppressFinalize(this);
    }
}
