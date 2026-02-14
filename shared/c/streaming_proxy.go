// Streaming proxy: forwards Watch, ReadRelationships, LookupResources, LookupSubjects to the in-memory server.
package main

import (
	"context"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"runtime"

	pb "github.com/authzed/authzed-go/proto/authzed/api/v1"
	"golang.org/x/sync/errgroup"
	"google.golang.org/grpc"
)

// streamingProxyAddr returns the address for the streaming proxy (Unix or TCP).
func streamingProxyAddr(id uint64) string {
	if runtime.GOOS == "windows" {
		return fmt.Sprintf("127.0.0.1:%d", 50151+int(id%5000))
	}
	return filepath.Join(os.TempDir(), fmt.Sprintf("spicedb-streaming-%d-%d.sock", os.Getpid(), id))
}

// permissionsStreamingProxy forwards only the streaming RPCs to the bufconn backend.
type permissionsStreamingProxy struct {
	pb.UnimplementedPermissionsServiceServer
	client pb.PermissionsServiceClient
}

func (p *permissionsStreamingProxy) ReadRelationships(req *pb.ReadRelationshipsRequest, srv grpc.ServerStreamingServer[pb.ReadRelationshipsResponse]) error {
	ctx := srv.Context()
	stream, err := p.client.ReadRelationships(ctx, req)
	if err != nil {
		return err
	}
	for {
		msg, err := stream.Recv()
		if err == io.EOF {
			return nil
		}
		if err != nil {
			return err
		}
		if err := srv.Send(msg); err != nil {
			return err
		}
	}
}

func (p *permissionsStreamingProxy) LookupResources(req *pb.LookupResourcesRequest, srv grpc.ServerStreamingServer[pb.LookupResourcesResponse]) error {
	ctx := srv.Context()
	stream, err := p.client.LookupResources(ctx, req)
	if err != nil {
		return err
	}
	for {
		msg, err := stream.Recv()
		if err == io.EOF {
			return nil
		}
		if err != nil {
			return err
		}
		if err := srv.Send(msg); err != nil {
			return err
		}
	}
}

func (p *permissionsStreamingProxy) LookupSubjects(req *pb.LookupSubjectsRequest, srv grpc.ServerStreamingServer[pb.LookupSubjectsResponse]) error {
	ctx := srv.Context()
	stream, err := p.client.LookupSubjects(ctx, req)
	if err != nil {
		return err
	}
	for {
		msg, err := stream.Recv()
		if err == io.EOF {
			return nil
		}
		if err != nil {
			return err
		}
		if err := srv.Send(msg); err != nil {
			return err
		}
	}
}

// watchStreamingProxy forwards Watch to the bufconn backend.
type watchStreamingProxy struct {
	pb.UnimplementedWatchServiceServer
	client pb.WatchServiceClient
	id     uint64 // instance id (for logging; same instance that owns client)
}

func (w *watchStreamingProxy) Watch(req *pb.WatchRequest, srv grpc.ServerStreamingServer[pb.WatchResponse]) error {
	ctx := srv.Context()
	stream, err := w.client.Watch(ctx, req)
	if err != nil {
		return err
	}
	for {
		msg, err := stream.Recv()
		if err == io.EOF {
			return nil
		}
		if err != nil {
			return err
		}
		if err := srv.Send(msg); err != nil {
			return err
		}
	}
}

// startStreamingProxy starts a gRPC server on a unix/tcp listener that forwards only streaming RPCs to the bufconn clientConn.
func startStreamingProxy(ctx context.Context, instance *Instance, id uint64, wg *errgroup.Group) (string, error) {
	addr := streamingProxyAddr(id)
	var lis net.Listener
	var err error
	if runtime.GOOS == "windows" {
		lis, err = net.Listen("tcp", addr)
	} else {
		_ = os.Remove(addr)
		lis, err = net.Listen("unix", addr)
	}
	if err != nil {
		return "", err
	}
	instance.streamingListener = lis

	permClient := pb.NewPermissionsServiceClient(instance.clientConn)
	watchClient := pb.NewWatchServiceClient(instance.clientConn)

	srv := grpc.NewServer()
	pb.RegisterPermissionsServiceServer(srv, &permissionsStreamingProxy{client: permClient})
	pb.RegisterWatchServiceServer(srv, &watchStreamingProxy{client: watchClient, id: id})
	instance.streamingServer = srv

	wg.Go(func() error {
		if err := srv.Serve(lis); err != nil && ctx.Err() == nil {
			return err
		}
		return nil
	})
	return addr, nil
}
