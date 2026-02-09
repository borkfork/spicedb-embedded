package com.borkfork.spicedb.embedded;

import io.grpc.ManagedChannel;
import io.grpc.netty.NettyChannelBuilder;
import io.netty.channel.EventLoopGroup;
import io.netty.channel.epoll.EpollDomainSocketChannel;
import io.netty.channel.epoll.EpollEventLoopGroup;
import io.netty.channel.kqueue.KQueueDomainSocketChannel;
import io.netty.channel.kqueue.KQueueEventLoopGroup;
import io.netty.channel.unix.DomainSocketAddress;
import java.io.File;

/** Builds a gRPC ManagedChannel that connects to a Unix domain socket (Linux/macOS). */
final class UnixSocketChannel {

  private UnixSocketChannel() {}

  static ManagedChannel build(String address) {
    // address is socket file path
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
