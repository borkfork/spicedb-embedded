// FFI RPC exports (permissions and schema) that call the in-memory gRPC client.
package main

/*
#include <stdlib.h>
*/
import "C"

import (
	"context"
	"fmt"
	"time"
	"unsafe"

	pb "github.com/authzed/authzed-go/proto/authzed/api/v1"
	"google.golang.org/grpc"
	"google.golang.org/grpc/metadata"
	"google.golang.org/protobuf/proto"
)

// runRPC is a helper for FFI RPCs: validate handle/transport, unmarshal req, call fn, marshal resp.
func runRPC(parent context.Context, handle C.ulonglong, reqBytes []byte, outResp **C.uchar, outLen *C.int, outErr **C.char,
	unmarshalReq func([]byte) (proto.Message, error),
	callRPC func(context.Context, *grpc.ClientConn, proto.Message) (proto.Message, error),
) {
	*outResp = nil
	*outLen = 0
	*outErr = nil

	instanceMu.RLock()
	instance, ok := instances[uint64(handle)]
	instanceMu.RUnlock()
	if !ok || instance == nil {
		*outErr = C.CString(fmt.Sprintf("invalid handle: %d", handle))
		return
	}
	if instance.clientConn == nil {
		*outErr = C.CString("instance not available for FFI RPCs")
		return
	}

	req, err := unmarshalReq(reqBytes)
	if err != nil {
		*outErr = C.CString(err.Error())
		return
	}

	ctx, cancel := context.WithTimeout(parent, 30*time.Second)
	defer cancel()
	resp, err := callRPC(ctx, instance.clientConn, req)
	if err != nil {
		*outErr = C.CString(err.Error())
		return
	}

	respBytes, err := proto.Marshal(resp)
	if err != nil {
		*outErr = C.CString(fmt.Sprintf("failed to marshal response: %v", err))
		return
	}
	n := len(respBytes)
	*outLen = C.int(n)
	if n > 0 {
		*outResp = (*C.uchar)(C.CBytes(respBytes))
	}
}

// rpcMessages maps each unary method spicedb_rpc accepts to new request and response messages.
var rpcMessages = map[string]func() (proto.Message, proto.Message){
	pb.PermissionsService_CheckPermission_FullMethodName: func() (proto.Message, proto.Message) {
		return &pb.CheckPermissionRequest{}, &pb.CheckPermissionResponse{}
	},
	pb.PermissionsService_CheckBulkPermissions_FullMethodName: func() (proto.Message, proto.Message) {
		return &pb.CheckBulkPermissionsRequest{}, &pb.CheckBulkPermissionsResponse{}
	},
	pb.PermissionsService_WriteRelationships_FullMethodName: func() (proto.Message, proto.Message) {
		return &pb.WriteRelationshipsRequest{}, &pb.WriteRelationshipsResponse{}
	},
	pb.PermissionsService_DeleteRelationships_FullMethodName: func() (proto.Message, proto.Message) {
		return &pb.DeleteRelationshipsRequest{}, &pb.DeleteRelationshipsResponse{}
	},
	pb.PermissionsService_ExpandPermissionTree_FullMethodName: func() (proto.Message, proto.Message) {
		return &pb.ExpandPermissionTreeRequest{}, &pb.ExpandPermissionTreeResponse{}
	},
	pb.SchemaService_ReadSchema_FullMethodName: func() (proto.Message, proto.Message) {
		return &pb.ReadSchemaRequest{}, &pb.ReadSchemaResponse{}
	},
	pb.SchemaService_WriteSchema_FullMethodName: func() (proto.Message, proto.Message) {
		return &pb.WriteSchemaRequest{}, &pb.WriteSchemaResponse{}
	},
}

// traceContext returns parent carrying a W3C traceparent and tracestate as gRPC metadata, so SpiceDB's
// server spans join the caller's trace. An empty traceparent leaves parent unchanged.
func traceContext(parent context.Context, traceparent, tracestate string) context.Context {
	if traceparent == "" {
		return parent
	}
	md := metadata.Pairs("traceparent", traceparent)
	if tracestate != "" {
		md.Set("tracestate", tracestate)
	}
	return metadata.NewOutgoingContext(parent, md)
}

// spicedb_rpc invokes a unary method by its full gRPC name, such as
// "/authzed.api.v1.PermissionsService/CheckPermission", under the caller's W3C trace context.
// traceparent and tracestate may be NULL. Responses and errors are returned as for the per-method exports.
//
//export spicedb_rpc
func spicedb_rpc(
	handle C.ulonglong,
	method *C.char,
	traceparent *C.char,
	tracestate *C.char,
	requestBytes *C.uchar,
	requestLen C.int,
	outResponseBytes **C.uchar,
	outResponseLen *C.int,
	outError **C.char,
) {
	name := C.GoString(method)
	messages, ok := rpcMessages[name]
	if !ok {
		*outResponseBytes = nil
		*outResponseLen = 0
		*outError = C.CString(fmt.Sprintf("unsupported method: %s", name))
		return
	}
	parent := traceContext(context.Background(), C.GoString(traceparent), C.GoString(tracestate))
	reqBytes := C.GoBytes(unsafe.Pointer(requestBytes), requestLen)
	req, resp := messages()
	runRPC(parent, handle, reqBytes, outResponseBytes, outResponseLen, outError,
		func(b []byte) (proto.Message, error) {
			if err := proto.Unmarshal(b, req); err != nil {
				return nil, err
			}
			return req, nil
		},
		func(ctx context.Context, conn *grpc.ClientConn, req proto.Message) (proto.Message, error) {
			if err := conn.Invoke(ctx, name, req, resp); err != nil {
				return nil, err
			}
			return resp, nil
		},
	)
}

// spicedb_free_bytes frees a byte buffer returned by the permissions/schema RPC FFI functions
// (out_response_bytes). Safe to call with NULL (no-op). Caller must call this for every buffer
// returned in *out_response_bytes.
//
//export spicedb_free_bytes
func spicedb_free_bytes(ptr *C.void) {
	if ptr != nil {
		C.free(unsafe.Pointer(ptr))
	}
}

// spicedb_permissions_check_bulk_permissions invokes PermissionsService.CheckBulkPermissions.
//
//export spicedb_permissions_check_bulk_permissions
func spicedb_permissions_check_bulk_permissions(
	handle C.ulonglong,
	requestBytes *C.uchar,
	requestLen C.int,
	outResponseBytes **C.uchar,
	outResponseLen *C.int,
	outError **C.char,
) {
	reqBytes := C.GoBytes(unsafe.Pointer(requestBytes), requestLen)
	runRPC(context.Background(), handle, reqBytes, outResponseBytes, outResponseLen, outError,
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

// spicedb_permissions_check_permission invokes PermissionsService.CheckPermission.
//
//export spicedb_permissions_check_permission
func spicedb_permissions_check_permission(
	handle C.ulonglong,
	requestBytes *C.uchar,
	requestLen C.int,
	outResponseBytes **C.uchar,
	outResponseLen *C.int,
	outError **C.char,
) {
	reqBytes := C.GoBytes(unsafe.Pointer(requestBytes), requestLen)
	runRPC(context.Background(), handle, reqBytes, outResponseBytes, outResponseLen, outError,
		func(b []byte) (proto.Message, error) {
			var req pb.CheckPermissionRequest
			if err := proto.Unmarshal(b, &req); err != nil {
				return nil, err
			}
			return &req, nil
		},
		func(ctx context.Context, conn *grpc.ClientConn, req proto.Message) (proto.Message, error) {
			return pb.NewPermissionsServiceClient(conn).CheckPermission(ctx, req.(*pb.CheckPermissionRequest))
		},
	)
}

// spicedb_permissions_delete_relationships invokes PermissionsService.DeleteRelationships.
//
//export spicedb_permissions_delete_relationships
func spicedb_permissions_delete_relationships(
	handle C.ulonglong,
	requestBytes *C.uchar,
	requestLen C.int,
	outResponseBytes **C.uchar,
	outResponseLen *C.int,
	outError **C.char,
) {
	reqBytes := C.GoBytes(unsafe.Pointer(requestBytes), requestLen)
	runRPC(context.Background(), handle, reqBytes, outResponseBytes, outResponseLen, outError,
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

// spicedb_permissions_expand_permission_tree invokes PermissionsService.ExpandPermissionTree.
//
//export spicedb_permissions_expand_permission_tree
func spicedb_permissions_expand_permission_tree(
	handle C.ulonglong,
	requestBytes *C.uchar,
	requestLen C.int,
	outResponseBytes **C.uchar,
	outResponseLen *C.int,
	outError **C.char,
) {
	reqBytes := C.GoBytes(unsafe.Pointer(requestBytes), requestLen)
	runRPC(context.Background(), handle, reqBytes, outResponseBytes, outResponseLen, outError,
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

// spicedb_permissions_write_relationships invokes PermissionsService.WriteRelationships.
//
//export spicedb_permissions_write_relationships
func spicedb_permissions_write_relationships(
	handle C.ulonglong,
	requestBytes *C.uchar,
	requestLen C.int,
	outResponseBytes **C.uchar,
	outResponseLen *C.int,
	outError **C.char,
) {
	reqBytes := C.GoBytes(unsafe.Pointer(requestBytes), requestLen)
	runRPC(context.Background(), handle, reqBytes, outResponseBytes, outResponseLen, outError,
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

// spicedb_schema_read_schema invokes SchemaService.ReadSchema.
//
//export spicedb_schema_read_schema
func spicedb_schema_read_schema(
	handle C.ulonglong,
	requestBytes *C.uchar,
	requestLen C.int,
	outResponseBytes **C.uchar,
	outResponseLen *C.int,
	outError **C.char,
) {
	reqBytes := C.GoBytes(unsafe.Pointer(requestBytes), requestLen)
	runRPC(context.Background(), handle, reqBytes, outResponseBytes, outResponseLen, outError,
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

// spicedb_schema_write_schema invokes SchemaService.WriteSchema.
//
//export spicedb_schema_write_schema
func spicedb_schema_write_schema(
	handle C.ulonglong,
	requestBytes *C.uchar,
	requestLen C.int,
	outResponseBytes **C.uchar,
	outResponseLen *C.int,
	outError **C.char,
) {
	reqBytes := C.GoBytes(unsafe.Pointer(requestBytes), requestLen)
	runRPC(context.Background(), handle, reqBytes, outResponseBytes, outResponseLen, outError,
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
