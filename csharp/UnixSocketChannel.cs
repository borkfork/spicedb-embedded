using System.Net.Sockets;

namespace Rendil.Spicedb.Embedded;

/// <summary>
/// Builds a gRPC SocketsHttpHandler that connects to a Unix domain socket.
/// </summary>
internal static class UnixSocketChannel
{
    public static SocketsHttpHandler CreateHandler(string address)
    {
        return new SocketsHttpHandler
        {
            ConnectCallback = async (ctx, ct) =>
            {
                var unixSocket = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.IP);
                var endpoint = new UnixDomainSocketEndPoint(address);
                await unixSocket.ConnectAsync(endpoint, ct);
                return new NetworkStream(unixSocket, ownsSocket: true);
            },
        };
    }
}
