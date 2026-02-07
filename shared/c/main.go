// Package main provides a C-shared library for embedding SpiceDB.
// Build with: go build -buildmode=c-shared -o libspicedb.so .
//
// This implementation creates a SpiceDB gRPC server on a Unix socket,
// allowing Rust to use native tonic/protobuf clients.
package main

/*
#include <stdlib.h>
*/
import "C"

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"sync/atomic"
	"unsafe"

	"github.com/authzed/spicedb/pkg/cmd/datastore"
	"github.com/authzed/spicedb/pkg/cmd/server"
	"github.com/authzed/spicedb/pkg/cmd/util"
	"golang.org/x/sync/errgroup"
)

// Instance holds a SpiceDB server instance
type Instance struct {
	server     server.RunnableServer
	socketPath string
	cancel     context.CancelFunc
	wg         *errgroup.Group
}

// Instance management
var (
	instanceMu sync.RWMutex
	instances  = make(map[uint64]*Instance)
	nextID     uint64
)

// Response is the JSON response format
type Response struct {
	Success bool            `json:"success"`
	Error   string          `json:"error,omitempty"`
	Data    json.RawMessage `json:"data,omitempty"`
}

func makeError(msg string) *C.char {
	resp := Response{Success: false, Error: msg}
	data, _ := json.Marshal(resp)
	return C.CString(string(data))
}

func makeSuccess(data interface{}) *C.char {
	var rawData json.RawMessage
	if data != nil {
		rawData, _ = json.Marshal(data)
	}
	resp := Response{Success: true, Data: rawData}
	respData, _ := json.Marshal(resp)
	return C.CString(string(respData))
}

// spicedb_free frees a string returned by other functions.
//
//export spicedb_free
func spicedb_free(ptr *C.char) {
	C.free(unsafe.Pointer(ptr))
}

// spicedb_start creates a new SpiceDB instance (empty server).
// Schema and relationships should be written by the caller via gRPC.
//
// Returns JSON response with handle and socket_path:
// {"success": true, "data": {"handle": 123, "socket_path": "/tmp/spicedb-xxx.sock"}}
//
//export spicedb_start
func spicedb_start() *C.char {
	id := atomic.AddUint64(&nextID, 1)
	socketPath := filepath.Join(os.TempDir(), fmt.Sprintf("spicedb-%d-%d.sock", os.Getpid(), id))
	os.Remove(socketPath)

	ctx, cancel := context.WithCancel(context.Background())

	srv, err := newSpiceDBServer(ctx, socketPath)
	if err != nil {
		cancel()
		return makeError(fmt.Sprintf("failed to create server: %v", err))
	}

	instance := &Instance{
		server:     srv,
		socketPath: socketPath,
		cancel:     cancel,
	}

	var wg errgroup.Group
	wg.Go(func() error {
		if err := srv.Run(ctx); err != nil && ctx.Err() == nil {
			return err
		}
		return nil
	})
	instance.wg = &wg

	instanceMu.Lock()
	instances[id] = instance
	instanceMu.Unlock()

	return makeSuccess(map[string]interface{}{
		"handle":      id,
		"socket_path": socketPath,
	})
}

// newSpiceDBServer creates a new in-memory SpiceDB server listening on a Unix socket
func newSpiceDBServer(ctx context.Context, socketPath string) (server.RunnableServer, error) {
	ds, err := datastore.NewDatastore(ctx,
		datastore.DefaultDatastoreConfig().ToOption(),
		datastore.WithRequestHedgingEnabled(false),
	)
	if err != nil {
		return nil, fmt.Errorf("unable to start memdb datastore: %v", err)
	}

	configOpts := []server.ConfigOption{
		server.WithGRPCServer(util.GRPCServerConfig{
			Network: "unix",
			Address: socketPath,
			Enabled: true,
		}),
		server.WithGRPCAuthFunc(func(ctx context.Context) (context.Context, error) {
			return ctx, nil
		}),
		server.WithHTTPGateway(util.HTTPServerConfig{HTTPEnabled: false}),
		server.WithMetricsAPI(util.HTTPServerConfig{HTTPEnabled: false}),
		server.WithDispatchCacheConfig(server.CacheConfig{Enabled: false, Metrics: false}),
		server.WithNamespaceCacheConfig(server.CacheConfig{Enabled: false, Metrics: false}),
		server.WithClusterDispatchCacheConfig(server.CacheConfig{Enabled: false, Metrics: false}),
		server.WithDatastore(ds),
	}

	return server.NewConfigWithOptionsAndDefaults(configOpts...).Complete(ctx)
}

// spicedb_dispose disposes of a SpiceDB instance.
//
//export spicedb_dispose
func spicedb_dispose(handle C.ulonglong) *C.char {
	id := uint64(handle)

	instanceMu.Lock()
	instance, ok := instances[id]
	if ok {
		delete(instances, id)
	}
	instanceMu.Unlock()

	if !ok {
		return makeError(fmt.Sprintf("invalid handle: %d", id))
	}

	// Cancel context and wait for server to stop
	instance.cancel()
	_ = instance.wg.Wait()

	// Clean up socket file
	os.Remove(instance.socketPath)

	return makeSuccess(nil)
}

// main is required but not used for c-shared build mode
func main() {}
