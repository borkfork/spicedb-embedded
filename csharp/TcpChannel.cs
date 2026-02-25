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
                string host;
                int port;
                if (address.StartsWith("["))
                {
                    // IPv6: [host]:port
                    var closeBracket = address.IndexOf(']');
                    if (closeBracket < 0 || closeBracket + 1 >= address.Length || address[closeBracket + 1] != ':')
                        throw new SpiceDbException("Invalid TCP address: " + address);
                    host = address.Substring(1, closeBracket - 1);
                    if (string.IsNullOrEmpty(host))
                        throw new SpiceDbException("Invalid TCP address (empty host): " + address);
                    if (!int.TryParse(address.Substring(closeBracket + 2), out port))
                        throw new SpiceDbException("Invalid TCP address (bad port): " + address);
                }
                else
                {
                    // IPv4 or hostname: host:port
                    var colonIdx = address.LastIndexOf(':');
                    if (colonIdx <= 0 || !int.TryParse(address.Substring(colonIdx + 1), out port))
                        throw new SpiceDbException("Invalid TCP address: " + address);
                    host = address.Substring(0, colonIdx);
                }

                var socket = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
                await socket.ConnectAsync(new DnsEndPoint(host, port), ct);
                return new NetworkStream(socket, true);
            }
        };
    }
}
