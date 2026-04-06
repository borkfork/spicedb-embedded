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
	"net/http"
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
	"github.com/prometheus/client_golang/prometheus/promhttp"
	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracegrpc"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	oteltrace "go.opentelemetry.io/otel/trace"
	"golang.org/x/sync/errgroup"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

// Instance holds a SpiceDB server instance (in-memory + streaming proxy).
type Instance struct {
	server             server.RunnableServer
	transport          string // always "memory"
	clientConn         *grpc.ClientConn
	streamingAddr      string
	streamingListener  net.Listener
	streamingServer    *grpc.Server
	cancel             context.CancelFunc
	wg                 *errgroup.Group
	tracerProvider     *sdktrace.TracerProvider
	prevTracerProvider oteltrace.TracerProvider
	metricsServer      *http.Server
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

// StartOptions configures the datastore and observability. Passed as JSON to spicedb_start.
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
//
// Observability (all disabled by default):
//   - metrics_enabled: primary switch for all metrics and tracing (default: false). When false, all
//     other metric/tracing options are ignored.
//   - datastore_metrics_enabled: enable datastore Prometheus metrics (default: true when metrics_enabled=true).
//   - cache_metrics_enabled: enable cache Prometheus metrics for dispatch/namespace/cluster caches
//     (default: true when metrics_enabled=true).
//   - otlp_endpoint: OTLP gRPC endpoint for OpenTelemetry traces, e.g. "localhost:4317". Insecure.
//     Only used when metrics_enabled=true.
//   - metrics_port: if > 0, starts a Prometheus HTTP server on this port at /metrics.
//     Only used when metrics_enabled=true.
type StartOptions struct {
	Datastore              string `json:"datastore"`
	DatastoreURI           string `json:"datastore_uri"`
	SpannerCredentialsFile string `json:"spanner_credentials_file"`
	SpannerEmulatorHost    string `json:"spanner_emulator_host"`
	MySQLTablePrefix       string `json:"mysql_table_prefix"`
	// MetricsEnabled is the primary switch. All other observability options are ignored when false.
	MetricsEnabled          bool   `json:"metrics_enabled"`
	// DatastoreMetricsEnabled enables datastore Prometheus metrics. Defaults to true when MetricsEnabled=true.
	// A null/absent value means "use the default" (true).
	DatastoreMetricsEnabled *bool  `json:"datastore_metrics_enabled,omitempty"`
	// CacheMetricsEnabled enables Prometheus metrics for internal caches. Defaults to true when MetricsEnabled=true.
	// A null/absent value means "use the default" (true).
	CacheMetricsEnabled     *bool  `json:"cache_metrics_enabled,omitempty"`
	// OTLPEndpoint, if non-empty, configures OpenTelemetry tracing to push to this OTLP gRPC endpoint.
	// Only used when MetricsEnabled=true. Example: "localhost:4317" (insecure).
	OTLPEndpoint            string `json:"otlp_endpoint,omitempty"`
	// MetricsPort, if > 0, starts a Prometheus HTTP server at /metrics on this port.
	// Only used when MetricsEnabled=true.
	MetricsPort             int    `json:"metrics_port,omitempty"`
	// MetricsHost is the host/IP the Prometheus HTTP server binds to (default: "0.0.0.0").
	// Only used when MetricsEnabled=true and MetricsPort > 0.
	MetricsHost             string `json:"metrics_host,omitempty"`
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
	srv, err := newSpiceDBServerFromDatastore(ctx, "", "memory", ds, opts.MetricsEnabled && boolPtrOrDefault(opts.CacheMetricsEnabled, true))
	if err != nil {
		cancel()
		return makeError(fmt.Sprintf("failed to create server: %v", err))
	}
	instance.server = srv
	instance.transport = "memory"

	if opts.MetricsEnabled && opts.OTLPEndpoint != "" {
		prev, tp, err := setupOTelTracing(ctx, opts.OTLPEndpoint)
		if err != nil {
			cancel()
			return makeError(fmt.Sprintf("failed to configure OTLP tracing: %v", err))
		}
		instance.tracerProvider = tp
		instance.prevTracerProvider = prev
	}

	if opts.MetricsEnabled && opts.MetricsPort > 0 {
		host := opts.MetricsHost
		if host == "" {
			host = "0.0.0.0"
		}
		srv, err := startMetricsServer(host, opts.MetricsPort)
		if err != nil {
			cancel()
			cleanupObservability(instance)
			return makeError(fmt.Sprintf("failed to start metrics server: %v", err))
		}
		instance.metricsServer = srv
	}

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
		cleanupObservability(instance)
		return makeError("failed to dial in-memory server")
	}
	proxyAddr, err := startStreamingProxy(ctx, instance, id, &wg)
	if err != nil {
		cancel()
		_ = wg.Wait()
		cleanupObservability(instance)
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
	datastoreMetrics := opts.MetricsEnabled && boolPtrOrDefault(opts.DatastoreMetricsEnabled, true)
	dsOpts := []datastore.ConfigOption{
		datastore.DefaultDatastoreConfig().ToOption(),
		datastore.WithEngine(engine),
		datastore.WithRequestHedgingEnabled(false),
		datastore.WithEnableDatastoreMetrics(datastoreMetrics),
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
// cacheMetricsEnabled controls whether internal cache metrics are registered.
func newSpiceDBServerFromDatastore(ctx context.Context, addr string, transport string, ds spicedbdatastore.Datastore, cacheMetricsEnabled bool) (server.RunnableServer, error) {
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
		server.WithDispatchCacheConfig(server.CacheConfig{Enabled: false, Metrics: cacheMetricsEnabled}),
		server.WithNamespaceCacheConfig(server.CacheConfig{Enabled: false, Metrics: cacheMetricsEnabled}),
		server.WithClusterDispatchCacheConfig(server.CacheConfig{Enabled: false, Metrics: cacheMetricsEnabled}),
		server.WithDatastore(ds),
	}
	return server.NewConfigWithOptionsAndDefaults(configOpts...).Complete(ctx)
}

// boolPtrOrDefault returns *b if non-nil, otherwise def.
func boolPtrOrDefault(b *bool, def bool) bool {
	if b == nil {
		return def
	}
	return *b
}

// setupOTelTracing configures the global OpenTelemetry TracerProvider to export via OTLP gRPC.
// endpoint should be "host:port" (e.g. "localhost:4317"). Uses insecure transport.
// Returns the previous global TracerProvider so it can be restored on shutdown.
func setupOTelTracing(ctx context.Context, endpoint string) (prev oteltrace.TracerProvider, tp *sdktrace.TracerProvider, err error) {
	exporter, err := otlptracegrpc.New(ctx,
		otlptracegrpc.WithEndpoint(endpoint),
		otlptracegrpc.WithInsecure(),
	)
	if err != nil {
		return nil, nil, fmt.Errorf("creating OTLP gRPC exporter: %w", err)
	}
	prev = otel.GetTracerProvider()
	tp = sdktrace.NewTracerProvider(sdktrace.WithBatcher(exporter))
	otel.SetTracerProvider(tp)
	return prev, tp, nil
}

// startMetricsServer binds to host:port and serves Prometheus metrics at /metrics.
// Returns an error immediately if the port cannot be bound.
func startMetricsServer(host string, port int) (*http.Server, error) {
	addr := fmt.Sprintf("%s:%d", host, port)
	ln, err := net.Listen("tcp", addr)
	if err != nil {
		return nil, fmt.Errorf("binding metrics port %d: %w", port, err)
	}
	mux := http.NewServeMux()
	mux.Handle("/metrics", promhttp.Handler())
	srv := &http.Server{
		Addr:    addr,
		Handler: mux,
	}
	go func() { _ = srv.Serve(ln) }()
	return srv, nil
}

// cleanupObservability shuts down the metrics HTTP server and tracer provider, restoring the
// previous global TracerProvider. Safe to call when either field is nil.
func cleanupObservability(instance *Instance) {
	if instance.metricsServer != nil {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = instance.metricsServer.Shutdown(ctx)
	}
	if instance.tracerProvider != nil {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = instance.tracerProvider.Shutdown(ctx)
		otel.SetTracerProvider(instance.prevTracerProvider)
	}
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

	cleanupObservability(instance)

	// Clean up streaming proxy socket file (Unix only)
	if instance.streamingAddr != "" && runtime.GOOS != "windows" {
		os.Remove(instance.streamingAddr)
	}

	return makeSuccess(nil)
}

// main is required but not used for c-shared build mode
func main() {}
