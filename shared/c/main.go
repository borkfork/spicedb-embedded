// Package main provides a C-shared library for embedding SpiceDB.
// Build with: go build -buildmode=c-shared -o libspicedb.so .
//
// The server uses an in-memory buffer (no main socket). Callers use FFI RPC functions
// (e.g. spicedb_permissions_check_permission) with marshalled protobuf bytes.
// spicedb_start returns a handle and streaming_address for Watch/ReadRelationships etc.
//
// Allocation audit: all C-visible allocations are caller-owned and must be freed.
//   - makeError/makeSuccess return C.CString → caller frees with spicedb_free (spicedb_start, spicedb_dispose).
//   - RPC FFI: *out_error = C.CString(...) → caller frees with spicedb_free; *out_response_bytes = C.CBytes(...) → caller frees with spicedb_free_bytes.
//   - Each code path sets at most one of out_error or out_response_bytes per call; no double-set, so no leak on our side.
package main

/*
#include <stdlib.h>
*/
import "C"

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"os"
	"runtime"
	"sync"
	"sync/atomic"
	"time"
	"unsafe"

	"github.com/authzed/spicedb/pkg/cmd/datastore"
	"github.com/authzed/spicedb/pkg/cmd/server"
	"github.com/authzed/spicedb/pkg/cmd/util"
	spicedbdatastore "github.com/authzed/spicedb/pkg/datastore"
	"golang.org/x/sync/errgroup"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

// Instance holds a SpiceDB server instance (in-memory + streaming proxy).
type Instance struct {
	server            server.RunnableServer
	transport         string // always "memory"
	clientConn        *grpc.ClientConn
	streamingAddr     string
	streamingListener net.Listener
	streamingServer   *grpc.Server
	cancel            context.CancelFunc
	wg                *errgroup.Group
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

// spicedb_free frees a string returned by spicedb_start or spicedb_dispose.
// Safe to call with NULL (no-op). Caller must call this for every *C.char returned by those functions.
//
//export spicedb_free
func spicedb_free(ptr *C.char) {
	if ptr != nil {
		C.free(unsafe.Pointer(ptr))
	}
}

// StartOptions configures the datastore. Passed as JSON to spicedb_start.
// Supported fields:
//   - datastore: "memory" (default), "postgres", "cockroachdb", "spanner", "mysql"
//   - datastore_uri: connection string for remote datastores (required for postgres, cockroachdb, spanner, mysql)
//
// Spanner-specific (when datastore=spanner):
//   - spanner_credentials_file: path to service account JSON key (omit for Application Default Credentials)
//   - spanner_emulator_host: e.g. "localhost:9010" for local Spanner emulator
//
// MySQL-specific (when datastore=mysql):
//   - mysql_table_prefix: prefix for all SpiceDB tables (optional, for multi-tenant)
//   - metrics_enabled: enable datastore Prometheus metrics (default: false; disabled allows multiple instances in same process)
type StartOptions struct {
	Datastore              string `json:"datastore"`
	DatastoreURI           string `json:"datastore_uri"`
	SpannerCredentialsFile string `json:"spanner_credentials_file"`
	SpannerEmulatorHost    string `json:"spanner_emulator_host"`
	MySQLTablePrefix       string `json:"mysql_table_prefix"`
	MetricsEnabled         bool   `json:"metrics_enabled"`
}

// spicedb_start creates a new SpiceDB instance (empty server).
// Schema and relationships should be written by the caller via gRPC.
//
// options_json: optional JSON string. Use NULL for defaults.
// Returns JSON: {"success": true, "data": {"handle": N, "grpc_transport": "memory", "streaming_address": "...", "streaming_transport": "unix"|"tcp"}}
// streaming_address: Unix path (when streaming_transport is "unix") or "127.0.0.1:port" (when "tcp").
//
//export spicedb_start
func spicedb_start(optionsJSON *C.char) *C.char {
	opts := parseStartOptions(optionsJSON)

	// Validate: remote datastores require datastore_uri
	engine := opts.Datastore
	if engine == "" {
		engine = datastore.MemoryEngine
	}
	remoteEngines := map[string]bool{
		"postgres": true, "cockroachdb": true, "spanner": true, "mysql": true,
	}
	if remoteEngines[engine] && opts.DatastoreURI == "" {
		return makeError(fmt.Sprintf("datastore_uri is required for %s datastore", engine))
	}

	ctx, cancel := context.WithCancel(context.Background())
	instance := &Instance{cancel: cancel}

	id := atomic.AddUint64(&nextID, 1)
	ds, err := newDatastoreFromOpts(ctx, opts)
	if err != nil {
		cancel()
		return makeError(fmt.Sprintf("failed to create datastore: %v", err))
	}
	srv, err := newSpiceDBServerFromDatastore(ctx, "", "memory", ds)
	if err != nil {
		cancel()
		return makeError(fmt.Sprintf("failed to create server: %v", err))
	}
	instance.server = srv
	instance.transport = "memory"

	var wg errgroup.Group
	wg.Go(func() error {
		if err := instance.server.Run(ctx); err != nil && ctx.Err() == nil {
			return err
		}
		return nil
	})
	instance.wg = &wg

	dialCtx, dialCancel := context.WithTimeout(ctx, 5*time.Second)
	defer dialCancel()
	for i := 0; i < 20; i++ {
		conn, err := instance.server.GRPCDialContext(dialCtx, grpc.WithTransportCredentials(insecure.NewCredentials()))
		if err == nil {
			instance.clientConn = conn
			break
		}
		time.Sleep(25 * time.Millisecond)
	}
	if instance.clientConn == nil {
		cancel()
		_ = wg.Wait()
		return makeError("failed to dial in-memory server")
	}
	proxyAddr, err := startStreamingProxy(ctx, instance, id, &wg)
	if err != nil {
		cancel()
		_ = wg.Wait()
		return makeError(fmt.Sprintf("failed to start streaming proxy: %v", err))
	}
	instance.streamingAddr = proxyAddr

	instanceMu.Lock()
	instances[id] = instance
	instanceMu.Unlock()

	streamingTransport := "unix"
	if runtime.GOOS == "windows" {
		streamingTransport = "tcp"
	}
	data := map[string]interface{}{
		"handle":              id,
		"grpc_transport":      "memory",
		"streaming_address":   instance.streamingAddr,
		"streaming_transport": streamingTransport,
	}
	return makeSuccess(data)
}

func parseStartOptions(optionsJSON *C.char) StartOptions {
	opts := StartOptions{}
	if optionsJSON == nil {
		return opts
	}
	s := C.GoString(optionsJSON)
	if s == "" {
		return opts
	}
	_ = json.Unmarshal([]byte(s), &opts)
	return opts
}

// newDatastoreFromOpts creates a datastore from StartOptions (used for memory transport in spicedb_start).
func newDatastoreFromOpts(ctx context.Context, opts StartOptions) (spicedbdatastore.Datastore, error) {
	engine := opts.Datastore
	if engine == "" {
		engine = datastore.MemoryEngine
	}
	dsOpts := []datastore.ConfigOption{
		datastore.DefaultDatastoreConfig().ToOption(),
		datastore.WithEngine(engine),
		datastore.WithRequestHedgingEnabled(false),
		datastore.WithEnableDatastoreMetrics(opts.MetricsEnabled),
	}
	if opts.DatastoreURI != "" {
		dsOpts = append(dsOpts, datastore.WithURI(opts.DatastoreURI))
	}
	if engine == "spanner" {
		if opts.SpannerCredentialsFile != "" {
			dsOpts = append(dsOpts, datastore.WithSpannerCredentialsFile(opts.SpannerCredentialsFile))
		}
		if opts.SpannerEmulatorHost != "" {
			dsOpts = append(dsOpts, datastore.WithSpannerEmulatorHost(opts.SpannerEmulatorHost))
		}
	}
	if engine == "mysql" && opts.MySQLTablePrefix != "" {
		dsOpts = append(dsOpts, datastore.WithTablePrefix(opts.MySQLTablePrefix))
	}
	return datastore.NewDatastore(ctx, dsOpts...)
}

// newSpiceDBServerFromDatastore creates a server that uses an existing datastore.
// transport: "memory" (bufconn), "unix", or "tcp". For memory, addr is ignored.
func newSpiceDBServerFromDatastore(ctx context.Context, addr string, transport string, ds spicedbdatastore.Datastore) (server.RunnableServer, error) {
	grpcConfig := util.GRPCServerConfig{Enabled: true}
	if transport == "memory" {
		grpcConfig.Network = util.BufferedNetwork
		grpcConfig.Address = ""
		grpcConfig.BufferSize = 1024 * 1024
	} else {
		network := "unix"
		if transport == "tcp" {
			network = "tcp"
		}
		grpcConfig.Network = network
		grpcConfig.Address = addr
	}
	configOpts := []server.ConfigOption{
		server.WithGRPCServer(grpcConfig),
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

	// Cancel context so main server Run() exits. Stop streaming proxy before wg.Wait(),
	// otherwise the proxy's Serve() never exits and wg.Wait() blocks forever.
	instance.cancel()

	if instance.clientConn != nil {
		_ = instance.clientConn.Close()
	}
	if instance.streamingServer != nil {
		instance.streamingServer.GracefulStop()
	}
	if instance.streamingListener != nil {
		_ = instance.streamingListener.Close()
	}

	_ = instance.wg.Wait()
	// Clean up streaming proxy socket file (Unix only)
	if instance.streamingAddr != "" && runtime.GOOS != "windows" {
		os.Remove(instance.streamingAddr)
	}

	return makeSuccess(nil)
}

// main is required but not used for c-shared build mode
func main() {}
