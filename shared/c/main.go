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
	"runtime"
	"strings"
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
	server    server.RunnableServer
	transport string // "unix" or "tcp"
	address   string // socket path or host:port
	cancel    context.CancelFunc
	wg        *errgroup.Group
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

// StartOptions configures datastore and transport. Passed as JSON to spicedb_start.
// Use nil or empty for defaults.
type StartOptions struct {
	Datastore         string `json:"datastore"`
	DatastoreURI      string `json:"datastore_uri"`
	GrpcTransport     string `json:"grpc_transport"`
	SpannerCredentialsFile string `json:"spanner_credentials_file"`
	SpannerEmulatorHost   string `json:"spanner_emulator_host"`
	MySQLTablePrefix  string `json:"mysql_table_prefix"`
}

// spicedb_start creates a new SpiceDB instance (empty server).
// Schema and relationships should be written by the caller via gRPC.
//
// options_json: optional JSON string. Use NULL for defaults.
// Returns JSON: {"success": true, "data": {"handle": N, "grpc_transport": "unix"|"tcp", "address": "..."}}
// Unix:   {"success": true, "data": {"handle": 123, "grpc_transport": "unix", "address": "/tmp/spicedb-xxx.sock"}}
// Windows: {"success": true, "data": {"handle": 123, "grpc_transport": "tcp", "address": "127.0.0.1:50051"}}
//
//export spicedb_start
func spicedb_start(options_json *C.char) *C.char {
	opts := parseStartOptions(options_json)

	ctx, cancel := context.WithCancel(context.Background())

	const maxRetries = 3
	var srv server.RunnableServer
	var addr string
	var id uint64
	var lastErr error

	transport := opts.GrpcTransport
	if transport == "" {
		if runtime.GOOS == "windows" {
			transport = "tcp"
		} else {
			transport = "unix"
		}
	}

	for attempt := 0; attempt < maxRetries; attempt++ {
		id = atomic.AddUint64(&nextID, 1)
		addr = listenAddr(id, transport)
		if transport == "unix" {
			os.Remove(addr)
		}
		srv, lastErr = newSpiceDBServer(ctx, addr, transport)
		if lastErr == nil {
			break
		}
		// Only retry on port-in-use; other errors are unlikely to succeed
		if !strings.Contains(lastErr.Error(), "address already in use") {
			break
		}
	}

	if lastErr != nil {
		cancel()
		return makeError(fmt.Sprintf("failed to create server: %v", lastErr))
	}

	instance := &Instance{
		server:    srv,
		transport: transport,
		address:   addr,
		cancel:    cancel,
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
		"handle":         id,
		"grpc_transport": transport,
		"address":        addr,
	})
}

func parseStartOptions(options_json *C.char) StartOptions {
	opts := StartOptions{}
	if options_json == nil {
		return opts
	}
	s := C.GoString(options_json)
	if s == "" {
		return opts
	}
	_ = json.Unmarshal([]byte(s), &opts)
	return opts
}

// listenAddr returns the address for a new instance (Unix socket path or TCP host:port).
func listenAddr(id uint64, transport string) string {
	if transport == "tcp" || runtime.GOOS == "windows" {
		return fmt.Sprintf("127.0.0.1:%d", 50051+(id%5000))
	}
	return filepath.Join(os.TempDir(), fmt.Sprintf("spicedb-%d-%d.sock", os.Getpid(), id))
}

// newSpiceDBServer creates a new in-memory SpiceDB server.
func newSpiceDBServer(ctx context.Context, addr string, transport string) (server.RunnableServer, error) {
	ds, err := datastore.NewDatastore(ctx,
		datastore.DefaultDatastoreConfig().ToOption(),
		datastore.WithRequestHedgingEnabled(false),
	)
	if err != nil {
		return nil, fmt.Errorf("unable to start memdb datastore: %v", err)
	}

	network := "unix"
	if transport == "tcp" || runtime.GOOS == "windows" {
		network = "tcp"
	}

	configOpts := []server.ConfigOption{
		server.WithGRPCServer(util.GRPCServerConfig{
			Network: network,
			Address: addr,
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

	// Clean up socket file (Unix only; TCP has nothing to remove)
	if instance.transport == "unix" {
		os.Remove(instance.address)
	}

	return makeSuccess(nil)
}

// main is required but not used for c-shared build mode
func main() {}
