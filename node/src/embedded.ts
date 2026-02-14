/**
 * Embedded SpiceDB instance.
 *
 * Uses the C-shared library with in-memory transport: unary RPCs go through FFI,
 * streaming APIs (Watch, ReadRelationships, etc.) use a streaming proxy.
 *
 * Use permissions(), schema(), and watch() to access the full SpiceDB API.
 */

import * as grpc from "@grpc/grpc-js";
import { v1 } from "@authzed/authzed-node";
import {
  spicedb_dispose,
  spicedb_schema_read_schema,
  spicedb_schema_write_schema,
  spicedb_permissions_check_permission,
  spicedb_permissions_write_relationships,
  spicedb_permissions_delete_relationships,
  spicedb_permissions_check_bulk_permissions,
  spicedb_permissions_expand_permission_tree,
  spicedb_start,
  type SpiceDBStartOptions,
} from "./ffi.js";
import { getTarget as getTcpTarget } from "./tcp_channel.js";
import { getTarget as getUnixSocketTarget } from "./unix_socket_channel.js";

const {
  CheckPermissionRequest,
  CheckPermissionResponse,
  CheckBulkPermissionsRequest,
  CheckBulkPermissionsResponse,
  ExpandPermissionTreeRequest,
  ExpandPermissionTreeResponse,
  DeleteRelationshipsRequest,
  DeleteRelationshipsResponse,
  WriteRelationshipsRequest,
  WriteRelationshipsResponse,
  ReadSchemaRequest,
  ReadSchemaResponse,
  WriteSchemaRequest,
  WriteSchemaResponse,
  NewClientWithChannelCredentials,
  RelationshipUpdate,
  RelationshipUpdate_Operation,
} = v1;

export class SpiceDBError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "SpiceDBError";
  }
}

/** Build gRPC target from streaming_address (Unix path or host:port). */
function streamingTarget(streamingAddress: string): string {
  return streamingAddress.startsWith("/")
    ? getUnixSocketTarget(streamingAddress)
    : getTcpTarget(streamingAddress);
}

export class EmbeddedSpiceDB {
  private readonly handle: number;
  private readonly _streamingAddress: string;
  private readonly streamingClient: v1.ZedClientInterface;
  private _closed = false;

  private constructor(
    handle: number,
    streamingAddr: string,
    streamingClient: v1.ZedClientInterface
  ) {
    this.handle = handle;
    this._streamingAddress = streamingAddr;
    this.streamingClient = streamingClient;
  }

  /**
   * Create a new embedded SpiceDB instance (in-memory only).
   *
   * @param schema - The SpiceDB schema definition (ZED language)
   * @param relationships - Initial relationships (empty array allowed)
   * @param options - Optional datastore options (no grpc_transport; always memory)
   * @returns New EmbeddedSpiceDB instance
   */
  static async create(
    schema: string,
    relationships: v1.Relationship[] = [],
    options?: SpiceDBStartOptions | null
  ): Promise<EmbeddedSpiceDB> {
    const data = spicedb_start(options ?? undefined);
    const { handle, streaming_address } = data;

    const target = streamingTarget(streaming_address);
    const creds = grpc.credentials.createInsecure();
    const streamingClient = NewClientWithChannelCredentials(target, creds, 0);

    const db = new EmbeddedSpiceDB(handle, streaming_address, streamingClient);

    try {
      await db.bootstrap(schema, relationships);
    } catch (e) {
      db.close();
      throw new SpiceDBError(
        `Failed to bootstrap: ${e instanceof Error ? e.message : String(e)}`
      );
    }

    return db;
  }

  private async bootstrap(
    schema: string,
    relationships: v1.Relationship[]
  ): Promise<void> {
    const schemaRequest = WriteSchemaRequest.create({ schema });
    const schemaBytes = new Uint8Array(
      WriteSchemaRequest.toBinary(schemaRequest) as ArrayLike<number>
    );
    spicedb_schema_write_schema(this.handle, schemaBytes);

    if (relationships.length > 0) {
      const updates = relationships.map((r) =>
        RelationshipUpdate.create({
          operation: RelationshipUpdate_Operation.TOUCH,
          relationship: r,
        })
      );
      const writeRequest = WriteRelationshipsRequest.create({ updates });
      const writeBytes = new Uint8Array(
        WriteRelationshipsRequest.toBinary(writeRequest) as ArrayLike<number>
      );
      spicedb_permissions_write_relationships(this.handle, writeBytes);
    }
  }

  /**
   * Permissions service: unary RPCs via FFI, streaming (ReadRelationships, LookupResources, LookupSubjects) via streaming proxy.
   */
  permissions(): v1.ZedClientInterface {
    const h = this.handle;
    const stream = this.streamingClient;

    const unaryCheckPermission = (
      input: v1.CheckPermissionRequest,
      metadata?: grpc.Metadata,
      options?: grpc.CallOptions,
      callback?: (
        err: grpc.ServiceError | null,
        value?: v1.CheckPermissionResponse
      ) => void
    ): grpc.ClientUnaryCall => {
      const bytes = new Uint8Array(
        CheckPermissionRequest.toBinary(input) as ArrayLike<number>
      );
      try {
        const out = spicedb_permissions_check_permission(h, bytes);
        const resp = CheckPermissionResponse.fromBinary(out);
        if (callback) {
          setImmediate(() => callback(null, resp));
        }
      } catch (err) {
        if (callback) {
          setImmediate(() => callback(err as grpc.ServiceError, undefined));
        }
      }
      return {} as grpc.ClientUnaryCall;
    };

    const unaryWriteRelationships = (
      input: v1.WriteRelationshipsRequest,
      metadata?: grpc.Metadata,
      options?: grpc.CallOptions,
      callback?: (
        err: grpc.ServiceError | null,
        value?: v1.WriteRelationshipsResponse
      ) => void
    ): grpc.ClientUnaryCall => {
      const bytes = new Uint8Array(
        WriteRelationshipsRequest.toBinary(input) as ArrayLike<number>
      );
      try {
        const out = spicedb_permissions_write_relationships(h, bytes);
        const resp = WriteRelationshipsResponse.fromBinary(out);
        if (callback) {
          setImmediate(() => callback(null, resp));
        }
      } catch (err) {
        if (callback) {
          setImmediate(() => callback(err as grpc.ServiceError, undefined));
        }
      }
      return {} as grpc.ClientUnaryCall;
    };

    const unaryDeleteRelationships = (
      input: v1.DeleteRelationshipsRequest,
      metadata?: grpc.Metadata,
      options?: grpc.CallOptions,
      callback?: (
        err: grpc.ServiceError | null,
        value?: v1.DeleteRelationshipsResponse
      ) => void
    ): grpc.ClientUnaryCall => {
      const bytes = new Uint8Array(
        DeleteRelationshipsRequest.toBinary(input) as ArrayLike<number>
      );
      try {
        const out = spicedb_permissions_delete_relationships(h, bytes);
        const resp = DeleteRelationshipsResponse.fromBinary(out);
        if (callback) {
          setImmediate(() => callback(null, resp));
        }
      } catch (err) {
        if (callback) {
          setImmediate(() => callback(err as grpc.ServiceError, undefined));
        }
      }
      return {} as grpc.ClientUnaryCall;
    };

    const unaryCheckBulkPermissions = (
      input: v1.CheckBulkPermissionsRequest,
      metadata?: grpc.Metadata,
      options?: grpc.CallOptions,
      callback?: (
        err: grpc.ServiceError | null,
        value?: v1.CheckBulkPermissionsResponse
      ) => void
    ): grpc.ClientUnaryCall => {
      const bytes = new Uint8Array(
        CheckBulkPermissionsRequest.toBinary(input) as ArrayLike<number>
      );
      try {
        const out = spicedb_permissions_check_bulk_permissions(h, bytes);
        const resp = CheckBulkPermissionsResponse.fromBinary(out);
        if (callback) {
          setImmediate(() => callback(null, resp));
        }
      } catch (err) {
        if (callback) {
          setImmediate(() => callback(err as grpc.ServiceError, undefined));
        }
      }
      return {} as grpc.ClientUnaryCall;
    };

    const unaryExpandPermissionTree = (
      input: v1.ExpandPermissionTreeRequest,
      metadata?: grpc.Metadata,
      options?: grpc.CallOptions,
      callback?: (
        err: grpc.ServiceError | null,
        value?: v1.ExpandPermissionTreeResponse
      ) => void
    ): grpc.ClientUnaryCall => {
      const bytes = new Uint8Array(
        ExpandPermissionTreeRequest.toBinary(input) as ArrayLike<number>
      );
      try {
        const out = spicedb_permissions_expand_permission_tree(h, bytes);
        const resp = ExpandPermissionTreeResponse.fromBinary(out);
        if (callback) {
          setImmediate(() => callback(null, resp));
        }
      } catch (err) {
        if (callback) {
          setImmediate(() => callback(err as grpc.ServiceError, undefined));
        }
      }
      return {} as grpc.ClientUnaryCall;
    };

    const promises = {
      ...stream.promises,
      readRelationships: stream.promises.readRelationships.bind(
        stream.promises
      ),
      lookupResources: stream.promises.lookupResources.bind(stream.promises),
      lookupSubjects: stream.promises.lookupSubjects.bind(stream.promises),
      checkPermission: (input: v1.CheckPermissionRequest) =>
        new Promise<v1.CheckPermissionResponse>((resolve, reject) => {
          unaryCheckPermission(input, undefined, undefined, (err, value) =>
            err ? reject(err) : resolve(value!)
          );
        }),
      writeRelationships: (input: v1.WriteRelationshipsRequest) =>
        new Promise<v1.WriteRelationshipsResponse>((resolve, reject) => {
          unaryWriteRelationships(input, undefined, undefined, (err, value) =>
            err ? reject(err) : resolve(value!)
          );
        }),
      deleteRelationships: (input: v1.DeleteRelationshipsRequest) =>
        new Promise<v1.DeleteRelationshipsResponse>((resolve, reject) => {
          unaryDeleteRelationships(input, undefined, undefined, (err, value) =>
            err ? reject(err) : resolve(value!)
          );
        }),
      checkBulkPermissions: (input: v1.CheckBulkPermissionsRequest) =>
        new Promise<v1.CheckBulkPermissionsResponse>((resolve, reject) => {
          unaryCheckBulkPermissions(
            input,
            undefined,
            undefined,
            (err, value) => (err ? reject(err) : resolve(value!))
          );
        }),
      expandPermissionTree: (input: v1.ExpandPermissionTreeRequest) =>
        new Promise<v1.ExpandPermissionTreeResponse>((resolve, reject) => {
          unaryExpandPermissionTree(
            input,
            undefined,
            undefined,
            (err, value) => (err ? reject(err) : resolve(value!))
          );
        }),
    };

    return new Proxy(stream, {
      get(target, prop) {
        if (prop === "promises") return promises;
        if (prop === "checkPermission") return unaryCheckPermission;
        if (prop === "writeRelationships") return unaryWriteRelationships;
        if (prop === "deleteRelationships") return unaryDeleteRelationships;
        if (prop === "checkBulkPermissions") return unaryCheckBulkPermissions;
        if (prop === "expandPermissionTree") return unaryExpandPermissionTree;
        return (target as Record<string, unknown>)[prop as string];
      },
    }) as unknown as v1.ZedClientInterface;
  }

  /**
   * Schema service (ReadSchema, WriteSchema, etc.) via FFI.
   */
  schema(): v1.ZedClientInterface {
    const h = this.handle;
    const stream = this.streamingClient;

    const unaryReadSchema = (
      input: v1.ReadSchemaRequest,
      metadata?: grpc.Metadata,
      options?: grpc.CallOptions,
      callback?: (
        err: grpc.ServiceError | null,
        value?: v1.ReadSchemaResponse
      ) => void
    ): grpc.ClientUnaryCall => {
      const bytes = new Uint8Array(
        ReadSchemaRequest.toBinary(input) as ArrayLike<number>
      );
      try {
        const out = spicedb_schema_read_schema(h, bytes);
        const resp = ReadSchemaResponse.fromBinary(out);
        if (callback) {
          setImmediate(() => callback(null, resp));
        }
      } catch (err) {
        if (callback) {
          setImmediate(() => callback(err as grpc.ServiceError, undefined));
        }
      }
      return {} as grpc.ClientUnaryCall;
    };

    const unaryWriteSchema = (
      input: v1.WriteSchemaRequest,
      metadata?: grpc.Metadata,
      options?: grpc.CallOptions,
      callback?: (
        err: grpc.ServiceError | null,
        value?: v1.WriteSchemaResponse
      ) => void
    ): grpc.ClientUnaryCall => {
      const bytes = new Uint8Array(
        WriteSchemaRequest.toBinary(input) as ArrayLike<number>
      );
      try {
        const out = spicedb_schema_write_schema(h, bytes);
        const resp = WriteSchemaResponse.fromBinary(out);
        if (callback) {
          setImmediate(() => callback(null, resp));
        }
      } catch (err) {
        if (callback) {
          setImmediate(() => callback(err as grpc.ServiceError, undefined));
        }
      }
      return {} as grpc.ClientUnaryCall;
    };

    const promises = {
      ...stream.promises,
      readSchema: (input: v1.ReadSchemaRequest) =>
        new Promise<v1.ReadSchemaResponse>((resolve, reject) => {
          unaryReadSchema(input, undefined, undefined, (err, value) =>
            err ? reject(err) : resolve(value!)
          );
        }),
      writeSchema: (input: v1.WriteSchemaRequest) =>
        new Promise<v1.WriteSchemaResponse>((resolve, reject) => {
          unaryWriteSchema(input, undefined, undefined, (err, value) =>
            err ? reject(err) : resolve(value!)
          );
        }),
    };

    return new Proxy(stream, {
      get(target, prop) {
        if (prop === "promises") return promises;
        if (prop === "readSchema") return unaryReadSchema;
        if (prop === "writeSchema") return unaryWriteSchema;
        return (target as Record<string, unknown>)[prop as string];
      },
    }) as unknown as v1.ZedClientInterface;
  }

  /**
   * Watch service (streaming) via streaming proxy.
   */
  watch(): v1.ZedClientInterface {
    return this.streamingClient;
  }

  /**
   * Streaming address (Unix path or host:port) for streaming APIs.
   */
  streamingAddress(): string {
    return this._streamingAddress;
  }

  /**
   * Dispose the instance and close the streaming channel.
   */
  close(): void {
    if (this._closed) return;
    this._closed = true;
    this.streamingClient.close();
    spicedb_dispose(this.handle);
  }
}
