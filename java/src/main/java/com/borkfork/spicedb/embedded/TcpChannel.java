package com.borkfork.spicedb.embedded;

import io.grpc.ManagedChannel;
import io.grpc.netty.NettyChannelBuilder;
import io.netty.channel.nio.NioEventLoopGroup;
import io.netty.channel.socket.nio.NioSocketChannel;
import java.net.InetSocketAddress;

/** Builds a gRPC ManagedChannel that connects via TCP (host:port). */
final class TcpChannel {

  private TcpChannel() {}

  static ManagedChannel build(String address) {
    String host;
    int port;
    if (address.startsWith("[")) {
      // IPv6: [host]:port
      int closeBracket = address.indexOf(']');
      if (closeBracket < 0) {
        throw new IllegalArgumentException("Invalid TCP address (missing ']'): " + address);
      }
      host = address.substring(1, closeBracket);
      if (host.isEmpty()) {
        throw new IllegalArgumentException("Invalid TCP address (empty host): " + address);
      }
      if (closeBracket + 1 >= address.length() || address.charAt(closeBracket + 1) != ':') {
        throw new IllegalArgumentException("Invalid TCP address (missing port): " + address);
      }
      try {
        port = Integer.parseInt(address.substring(closeBracket + 2));
      } catch (NumberFormatException e) {
        throw new IllegalArgumentException("Invalid TCP address (bad port): " + address, e);
      }
    } else {
      // IPv4 or hostname: host:port
      int colon = address.lastIndexOf(':');
      if (colon <= 0) {
        throw new IllegalArgumentException("Invalid TCP address: " + address);
      }
      host = address.substring(0, colon);
      try {
        port = Integer.parseInt(address.substring(colon + 1));
      } catch (NumberFormatException e) {
        throw new IllegalArgumentException("Invalid TCP address (bad port): " + address, e);
      }
    }
    return NettyChannelBuilder.forAddress(new InetSocketAddress(host, port))
        .eventLoopGroup(new NioEventLoopGroup())
        .channelType(NioSocketChannel.class)
        .usePlaintext()
        .build();
  }
}
