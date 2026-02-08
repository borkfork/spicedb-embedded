package com.rendil.spicedb.embedded;

import io.grpc.ManagedChannel;
import io.grpc.netty.NettyChannelBuilder;
import io.netty.channel.EventLoopGroup;
import io.netty.channel.epoll.EpollDomainSocketChannel;
import io.netty.channel.epoll.EpollEventLoopGroup;
import io.netty.channel.kqueue.KQueueDomainSocketChannel;
import io.netty.channel.kqueue.KQueueEventLoopGroup;
import io.netty.channel.nio.NioEventLoopGroup;
import io.netty.channel.unix.DomainSocketAddress;
import java.io.File;
import java.net.InetSocketAddress;

/**
 * Builds a gRPC ManagedChannel that connects to a Unix domain socket (Linux/macOS) or TCP
 * (Windows).
 */
final class UnixSocketChannel {

  private UnixSocketChannel() {}

  static ManagedChannel build(String transport, String address) {
    if ("tcp".equals(transport)) {
      int colon = address.lastIndexOf(':');
      if (colon > 0) {
        String host = address.substring(0, colon);
        int port = Integer.parseInt(address.substring(colon + 1));
        return NettyChannelBuilder.forAddress(new InetSocketAddress(host, port))
            .eventLoopGroup(new NioEventLoopGroup())
            .usePlaintext()
            .build();
      }
      throw new IllegalArgumentException("Invalid TCP address: " + address);
    }

    // Unix: address is socket file path
    File f = new File(address);
    if (!f.exists()) {
      throw new IllegalArgumentException("Socket path does not exist: " + address);
    }
    DomainSocketAddress domainAddr = new DomainSocketAddress(address);
    String os = System.getProperty("os.name").toLowerCase();

    if (os.contains("linux")) {
      EventLoopGroup group = new EpollEventLoopGroup();
      return NettyChannelBuilder.forAddress(domainAddr)
          .eventLoopGroup(group)
          .channelType(EpollDomainSocketChannel.class)
          .usePlaintext()
          .build();
    } else if (os.contains("mac")) {
      EventLoopGroup group = new KQueueEventLoopGroup();
      return NettyChannelBuilder.forAddress(domainAddr)
          .eventLoopGroup(group)
          .channelType(KQueueDomainSocketChannel.class)
          .usePlaintext()
          .build();
    } else {
      throw new UnsupportedOperationException("Unix domain sockets not supported on " + os);
    }
  }
}
