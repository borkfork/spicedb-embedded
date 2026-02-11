using Authzed.Api.V1;
using Grpc.Core;
using Grpc.Net.Client;

namespace Borkfork.SpiceDb.Embedded;

/// <summary>
///     Embedded SpiceDB instance.
///     A thin wrapper that starts SpiceDB via the C-shared library, connects over a Unix
///     socket, and bootstraps schema and relationships via gRPC.
///     Use Permissions(), Schema(), and Watch() to access the full SpiceDB API.
/// </summary>
public sealed class EmbeddedSpiceDb : IDisposable
{
    private readonly ulong _handle;
    private bool _disposed;

    private EmbeddedSpiceDb(ulong handle, GrpcChannel channel)
    {
        _handle = handle;
        Channel = channel;
    }

    /// <summary>
    ///     Underlying gRPC channel for custom usage.
    /// </summary>
    // ReSharper disable once MemberCanBePrivate.Global -- Public API for custom gRPC usage
    public GrpcChannel Channel { get; }

    /// <inheritdoc />
    public void Dispose()
    {
        if (_disposed) return;
        SpiceDbFfi.Dispose(_handle);
        Channel.Dispose();
        _disposed = true;
    }

    /// <summary>
    ///     Create a new embedded SpiceDB instance with a schema, relationships, and options.
    /// </summary>
    /// <param name="schema">The SpiceDB schema definition (ZED language)</param>
    /// <param name="relationships">Initial relationships (empty list or null allowed)</param>
    /// <param name="options">Optional configuration (datastore, grpc_transport). Pass null for defaults.</param>
    /// <returns>New EmbeddedSpiceDb instance</returns>
    public static EmbeddedSpiceDb Create(
        string schema,
        IReadOnlyList<Relationship>? relationships = null,
        StartOptions? options = null)
    {
        var (handle, grpcTransport, address) = SpiceDbFfi.Start(options);

        var httpHandler = grpcTransport == "tcp"
            ? TcpChannel.CreateHandler(address)
            : UnixSocketChannel.CreateHandler(address);

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
            try
            {
                SpiceDbFfi.Dispose(handle);
            }
            catch
            {
                /* best effort */
            }

            throw new SpiceDbException("Failed to connect to SpiceDB: " + ex.Message, ex);
        }

        var db = new EmbeddedSpiceDb(handle, channel);

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

    private void Bootstrap(string schema, IReadOnlyList<Relationship> relationships)
    {
        var schemaClient = new SchemaService.SchemaServiceClient(Channel);
        schemaClient.WriteSchema(new WriteSchemaRequest { Schema = schema });

        if (relationships.Count > 0)
        {
            var permClient = new PermissionsService.PermissionsServiceClient(Channel);
            var updates = relationships.Select(r => new RelationshipUpdate
            {
                Operation = RelationshipUpdate.Types.Operation.Touch,
                Relationship = r
            }).ToList();
            permClient.WriteRelationships(new WriteRelationshipsRequest { Updates = { updates } });
        }
    }

    /// <summary>
    ///     Permissions service client (CheckPermission, WriteRelationships, ReadRelationships, etc.).
    /// </summary>
    public PermissionsService.PermissionsServiceClient Permissions()
    {
        return new PermissionsService.PermissionsServiceClient(Channel);
    }

    /// <summary>
    ///     Schema service client (ReadSchema, WriteSchema, ReflectSchema, etc.).
    /// </summary>
    // ReSharper disable once UnusedMember.Global -- Public API
    public SchemaService.SchemaServiceClient Schema()
    {
        return new SchemaService.SchemaServiceClient(Channel);
    }

    /// <summary>
    ///     Watch service client for relationship changes.
    /// </summary>
    // ReSharper disable once UnusedMember.Global -- Public API
    public WatchService.WatchServiceClient Watch()
    {
        return new WatchService.WatchServiceClient(Channel);
    }
}
