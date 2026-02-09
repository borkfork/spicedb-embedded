package com.rendil.spicedb.embedded;

import io.grpc.ManagedChannel;
import io.grpc.netty.NettyChannelBuilder;
import io.netty.channel.nio.NioEventLoopGroup;
import io.netty.channel.socket.nio.NioSocketChannel;
import java.net.InetSocketAddress;

/** Builds a gRPC ManagedChannel that connects via TCP (host:port). */
final class TcpChannel {

  private TcpChannel() {}

  static ManagedChannel build(String address) {
    int colon = address.lastIndexOf(':');
    if (colon > 0) {
      String host = address.substring(0, colon);
      int port = Integer.parseInt(address.substring(colon + 1));
      return NettyChannelBuilder.forAddress(new InetSocketAddress(host, port))
          .eventLoopGroup(new NioEventLoopGroup())
          .channelType(NioSocketChannel.class)
          .usePlaintext()
          .build();
    }
    throw new IllegalArgumentException("Invalid TCP address: " + address);
  }
}
