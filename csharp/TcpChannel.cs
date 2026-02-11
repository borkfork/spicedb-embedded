using System.Net;
using System.Net.Sockets;

namespace Borkfork.SpiceDb.Embedded;

/// <summary>
///     Builds a gRPC SocketsHttpHandler that connects via TCP (host:port).
/// </summary>
internal static class TcpChannel
{
    public static SocketsHttpHandler CreateHandler(string address)
    {
        return new SocketsHttpHandler
        {
            ConnectCallback = async (_, ct) =>
            {
                var colonIdx = address.LastIndexOf(':');
                if (colonIdx > 0 && int.TryParse(address.Substring(colonIdx + 1), out var port))
                {
                    var host = address.Substring(0, colonIdx);
                    var socket = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
                    await socket.ConnectAsync(new DnsEndPoint(host, port), ct);
                    return new NetworkStream(socket, true);
                }

                throw new SpiceDbException("Invalid TCP address: " + address);
            }
        };
    }
}