// Package main provides a C-shared library for embedding SpiceDB.
// Build with: go build -buildmode=c-shared -o libspicedb.so .
//
// When grpc_transport is "memory", the server uses an in-memory buffer (no socket).
// Callers then use the FFI RPC functions (e.g. spicedb_permissions_check_permission)
// with marshalled protobuf bytes. Other transports ("unix", "tcp") work as before:
// spicedb_start returns an address and callers connect via gRPC in their language.
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
	"sync"
	"sync/atomic"
	"time"
	"unsafe"

	pb "github.com/authzed/authzed-go/proto/authzed/api/v1"
	"github.com/authzed/spicedb/pkg/cmd/datastore"
	"github.com/authzed/spicedb/pkg/cmd/server"
	"github.com/authzed/spicedb/pkg/cmd/util"
	"golang.org/x/sync/errgroup"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/protobuf/proto"
)

// Instance holds a SpiceDB server instance
type Instance struct {
	server     server.RunnableServer
	transport  string // "unix", "tcp", or "memory"
	address    string // socket path or host:port (empty for memory)
	clientConn *grpc.ClientConn // non-nil only when transport == "memory"
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

// StartOptions configures datastore and transport. Passed as JSON to spicedb_start.
// Supported fields:
//   - datastore: "memory" (default), "postgres", "cockroachdb", "spanner", "mysql"
//   - datastore_uri: connection string for remote datastores (required for postgres, cockroachdb, spanner, mysql)
//   - grpc_transport: "unix" (default on Unix), "tcp" (default on Windows), "memory" (in-memory; use FFI RPCs only)
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
	GrpcTransport          string `json:"grpc_transport"`
	SpannerCredentialsFile string `json:"spanner_credentials_file"`
	SpannerEmulatorHost    string `json:"spanner_emulator_host"`
	MySQLTablePrefix       string `json:"mysql_table_prefix"`
	MetricsEnabled         bool   `json:"metrics_enabled"`
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
		if transport != "memory" {
			addr = listenAddr(id, transport)
			if transport == "unix" {
				os.Remove(addr)
			}
		}
		srv, lastErr = newSpiceDBServer(ctx, addr, transport, opts)
		if lastErr == nil {
			break
		}
		// Retry with a new address (e.g. different port) on any error
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

	// For memory transport, dial the in-process server and store the client conn for FFI RPCs.
	if transport == "memory" {
		dialCtx, dialCancel := context.WithTimeout(ctx, 5*time.Second)
		defer dialCancel()
		for i := 0; i < 20; i++ {
			conn, err := srv.GRPCDialContext(dialCtx, grpc.WithTransportCredentials(insecure.NewCredentials()))
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
	}

	instanceMu.Lock()
	instances[id] = instance
	instanceMu.Unlock()

	data := map[string]interface{}{"handle": id, "grpc_transport": transport}
	if transport != "memory" {
		data["address"] = addr
	}
	return makeSuccess(data)
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

// newSpiceDBServer creates a new SpiceDB server with the given options.
func newSpiceDBServer(ctx context.Context, addr string, transport string, opts StartOptions) (server.RunnableServer, error) {
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

	ds, err := datastore.NewDatastore(ctx, dsOpts...)
	if err != nil {
		return nil, fmt.Errorf("unable to start %s datastore: %v", engine, err)
	}

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

	// Cancel context and wait for server to stop
	instance.cancel()
	_ = instance.wg.Wait()

	if instance.clientConn != nil {
		_ = instance.clientConn.Close()
	}
	// Clean up socket file (Unix only; TCP has nothing to remove)
	if instance.transport == "unix" {
		os.Remove(instance.address)
	}

	return makeSuccess(nil)
}

// spicedb_free_bytes frees a byte buffer returned by the permissions/schema RPC FFI functions.
//
//export spicedb_free_bytes
func spicedb_free_bytes(ptr *C.void) {
	if ptr != nil {
		C.free(unsafe.Pointer(ptr))
	}
}

// spicedb_permissions_check_permission invokes PermissionsService.CheckPermission.
// handle: from spicedb_start (must have been started with grpc_transport "memory").
// request_bytes/request_len: marshalled authzed.api.v1.CheckPermissionRequest (protobuf).
// On success: *out_response_bytes and *out_response_len are set; *out_error is NULL. Caller frees *out_response_bytes with spicedb_free_bytes.
// On error: *out_response_bytes is NULL, *out_response_len is 0, *out_error is set. Caller frees *out_error with spicedb_free.
//
//export spicedb_permissions_check_permission
func spicedb_permissions_check_permission(
	handle C.ulonglong,
	request_bytes *C.uchar,
	request_len C.int,
	out_response_bytes **C.uchar,
	out_response_len *C.int,
	out_error **C.char,
) {
	*out_response_bytes = nil
	*out_response_len = 0
	*out_error = nil

	instanceMu.RLock()
	instance, ok := instances[uint64(handle)]
	instanceMu.RUnlock()
	if !ok || instance == nil {
		*out_error = C.CString(fmt.Sprintf("invalid handle: %d", handle))
		return
	}
	if instance.clientConn == nil {
		*out_error = C.CString("instance not using memory transport; start with grpc_transport \"memory\" to use FFI RPCs")
		return
	}

	reqBytes := C.GoBytes(unsafe.Pointer(request_bytes), request_len)
	var req pb.CheckPermissionRequest
	if err := proto.Unmarshal(reqBytes, &req); err != nil {
		*out_error = C.CString(fmt.Sprintf("failed to unmarshal CheckPermissionRequest: %v", err))
		return
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	client := pb.NewPermissionsServiceClient(instance.clientConn)
	resp, err := client.CheckPermission(ctx, &req)
	if err != nil {
		*out_error = C.CString(err.Error())
		return
	}

	respBytes, err := proto.Marshal(resp)
	if err != nil {
		*out_error = C.CString(fmt.Sprintf("failed to marshal CheckPermissionResponse: %v", err))
		return
	}
	n := len(respBytes)
	*out_response_len = C.int(n)
	if n > 0 {
		*out_response_bytes = (*C.uchar)(C.CBytes(respBytes))
	}
}

// runRPC is a helper for FFI RPCs: validate handle/transport, unmarshal req, call fn, marshal resp.
func runRPC(handle C.ulonglong, requestBytes []byte, outResponseBytes **C.uchar, outResponseLen *C.int, outError **C.char,
	unmarshalReq func([]byte) (proto.Message, error),
	callRPC func(context.Context, *grpc.ClientConn, proto.Message) (proto.Message, error),
) {
	*outResponseBytes = nil
	*outResponseLen = 0
	*outError = nil

	instanceMu.RLock()
	instance, ok := instances[uint64(handle)]
	instanceMu.RUnlock()
	if !ok || instance == nil {
		*outError = C.CString(fmt.Sprintf("invalid handle: %d", handle))
		return
	}
	if instance.clientConn == nil {
		*outError = C.CString("instance not using memory transport; start with grpc_transport \"memory\" to use FFI RPCs")
		return
	}

	req, err := unmarshalReq(requestBytes)
	if err != nil {
		*outError = C.CString(err.Error())
		return
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	resp, err := callRPC(ctx, instance.clientConn, req)
	if err != nil {
		*outError = C.CString(err.Error())
		return
	}

	respBytes, err := proto.Marshal(resp)
	if err != nil {
		*outError = C.CString(fmt.Sprintf("failed to marshal response: %v", err))
		return
	}
	n := len(respBytes)
	*outResponseLen = C.int(n)
	if n > 0 {
		*outResponseBytes = (*C.uchar)(C.CBytes(respBytes))
	}
}

// spicedb_schema_write_schema invokes SchemaService.WriteSchema.
//
//export spicedb_schema_write_schema
func spicedb_schema_write_schema(
	handle C.ulonglong,
	request_bytes *C.uchar,
	request_len C.int,
	out_response_bytes **C.uchar,
	out_response_len *C.int,
	out_error **C.char,
) {
	reqBytes := C.GoBytes(unsafe.Pointer(request_bytes), request_len)
	runRPC(handle, reqBytes, out_response_bytes, out_response_len, out_error,
		func(b []byte) (proto.Message, error) {
			var req pb.WriteSchemaRequest
			if err := proto.Unmarshal(b, &req); err != nil {
				return nil, err
			}
			return &req, nil
		},
		func(ctx context.Context, conn *grpc.ClientConn, req proto.Message) (proto.Message, error) {
			return pb.NewSchemaServiceClient(conn).WriteSchema(ctx, req.(*pb.WriteSchemaRequest))
		},
	)
}

// spicedb_permissions_write_relationships invokes PermissionsService.WriteRelationships.
//
//export spicedb_permissions_write_relationships
func spicedb_permissions_write_relationships(
	handle C.ulonglong,
	request_bytes *C.uchar,
	request_len C.int,
	out_response_bytes **C.uchar,
	out_response_len *C.int,
	out_error **C.char,
) {
	reqBytes := C.GoBytes(unsafe.Pointer(request_bytes), request_len)
	runRPC(handle, reqBytes, out_response_bytes, out_response_len, out_error,
		func(b []byte) (proto.Message, error) {
			var req pb.WriteRelationshipsRequest
			if err := proto.Unmarshal(b, &req); err != nil {
				return nil, err
			}
			return &req, nil
		},
		func(ctx context.Context, conn *grpc.ClientConn, req proto.Message) (proto.Message, error) {
			return pb.NewPermissionsServiceClient(conn).WriteRelationships(ctx, req.(*pb.WriteRelationshipsRequest))
		},
	)
}

// spicedb_permissions_delete_relationships invokes PermissionsService.DeleteRelationships.
//
//export spicedb_permissions_delete_relationships
func spicedb_permissions_delete_relationships(
	handle C.ulonglong,
	request_bytes *C.uchar,
	request_len C.int,
	out_response_bytes **C.uchar,
	out_response_len *C.int,
	out_error **C.char,
) {
	reqBytes := C.GoBytes(unsafe.Pointer(request_bytes), request_len)
	runRPC(handle, reqBytes, out_response_bytes, out_response_len, out_error,
		func(b []byte) (proto.Message, error) {
			var req pb.DeleteRelationshipsRequest
			if err := proto.Unmarshal(b, &req); err != nil {
				return nil, err
			}
			return &req, nil
		},
		func(ctx context.Context, conn *grpc.ClientConn, req proto.Message) (proto.Message, error) {
			return pb.NewPermissionsServiceClient(conn).DeleteRelationships(ctx, req.(*pb.DeleteRelationshipsRequest))
		},
	)
}

// spicedb_permissions_check_bulk_permissions invokes PermissionsService.CheckBulkPermissions.
//
//export spicedb_permissions_check_bulk_permissions
func spicedb_permissions_check_bulk_permissions(
	handle C.ulonglong,
	request_bytes *C.uchar,
	request_len C.int,
	out_response_bytes **C.uchar,
	out_response_len *C.int,
	out_error **C.char,
) {
	reqBytes := C.GoBytes(unsafe.Pointer(request_bytes), request_len)
	runRPC(handle, reqBytes, out_response_bytes, out_response_len, out_error,
		func(b []byte) (proto.Message, error) {
			var req pb.CheckBulkPermissionsRequest
			if err := proto.Unmarshal(b, &req); err != nil {
				return nil, err
			}
			return &req, nil
		},
		func(ctx context.Context, conn *grpc.ClientConn, req proto.Message) (proto.Message, error) {
			return pb.NewPermissionsServiceClient(conn).CheckBulkPermissions(ctx, req.(*pb.CheckBulkPermissionsRequest))
		},
	)
}

// spicedb_permissions_expand_permission_tree invokes PermissionsService.ExpandPermissionTree.
//
//export spicedb_permissions_expand_permission_tree
func spicedb_permissions_expand_permission_tree(
	handle C.ulonglong,
	request_bytes *C.uchar,
	request_len C.int,
	out_response_bytes **C.uchar,
	out_response_len *C.int,
	out_error **C.char,
) {
	reqBytes := C.GoBytes(unsafe.Pointer(request_bytes), request_len)
	runRPC(handle, reqBytes, out_response_bytes, out_response_len, out_error,
		func(b []byte) (proto.Message, error) {
			var req pb.ExpandPermissionTreeRequest
			if err := proto.Unmarshal(b, &req); err != nil {
				return nil, err
			}
			return &req, nil
		},
		func(ctx context.Context, conn *grpc.ClientConn, req proto.Message) (proto.Message, error) {
			return pb.NewPermissionsServiceClient(conn).ExpandPermissionTree(ctx, req.(*pb.ExpandPermissionTreeRequest))
		},
	)
}

// spicedb_schema_read_schema invokes SchemaService.ReadSchema.
//
//export spicedb_schema_read_schema
func spicedb_schema_read_schema(
	handle C.ulonglong,
	request_bytes *C.uchar,
	request_len C.int,
	out_response_bytes **C.uchar,
	out_response_len *C.int,
	out_error **C.char,
) {
	reqBytes := C.GoBytes(unsafe.Pointer(request_bytes), request_len)
	runRPC(handle, reqBytes, out_response_bytes, out_response_len, out_error,
		func(b []byte) (proto.Message, error) {
			var req pb.ReadSchemaRequest
			if err := proto.Unmarshal(b, &req); err != nil {
				return nil, err
			}
			return &req, nil
		},
		func(ctx context.Context, conn *grpc.ClientConn, req proto.Message) (proto.Message, error) {
			return pb.NewSchemaServiceClient(conn).ReadSchema(ctx, req.(*pb.ReadSchemaRequest))
		},
	)
}

// main is required but not used for c-shared build mode
func main() {}
