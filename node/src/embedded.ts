/**
 * Embedded SpiceDB instance.
 *
 * A thin wrapper that starts SpiceDB via the C-shared library, connects over a Unix
 * socket, and bootstraps schema and relationships via gRPC.
 *
 * Use permissions(), schema(), and watch() to access the full SpiceDB API.
 */

import * as grpc from "@grpc/grpc-js";
import { v1 } from "@authzed/authzed-node";
import { spicedb_dispose, spicedb_start } from "./ffi.js";
import { getTarget as getTcpTarget } from "./tcp_channel.js";
import { getTarget as getUnixSocketTarget } from "./unix_socket_channel.js";

const {
  NewClientWithChannelCredentials,
  RelationshipUpdate,
  RelationshipUpdate_Operation,
  WriteRelationshipsRequest,
  WriteSchemaRequest,
} = v1;

export class SpiceDBError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "SpiceDBError";
  }
}

export class EmbeddedSpiceDB {
  private readonly handle: number;
  private readonly client: v1.ZedClientInterface;

  private constructor(handle: number, client: v1.ZedClientInterface) {
    this.handle = handle;
    this.client = client;
  }

  /**
   * Create a new embedded SpiceDB instance with a schema and relationships.
   *
   * @param schema - The SpiceDB schema definition (ZED language)
   * @param relationships - Initial relationships (empty array allowed)
   * @returns New EmbeddedSpiceDB instance
   */
  static async create(
    schema: string,
    relationships: v1.Relationship[] = []
  ): Promise<EmbeddedSpiceDB> {
    const data = spicedb_start();
    const { handle, grpc_transport, address } = data;

    const target =
      grpc_transport === "unix"
        ? getUnixSocketTarget(address)
        : getTcpTarget(address);
    const creds = grpc.credentials.createInsecure();
    const client = NewClientWithChannelCredentials(target, creds);

    const db = new EmbeddedSpiceDB(handle, client);

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
    await this.client.promises.writeSchema(schemaRequest);

    if (relationships.length > 0) {
      const updates = relationships.map((r) =>
        RelationshipUpdate.create({
          operation: RelationshipUpdate_Operation.TOUCH,
          relationship: r,
        })
      );
      const writeRequest = WriteRelationshipsRequest.create({
        updates,
      });
      await this.client.promises.writeRelationships(writeRequest);
    }
  }

  /**
   * Permissions service client (CheckPermission, WriteRelationships, ReadRelationships, etc.)
   */
  permissions(): v1.ZedClientInterface {
    return this.client;
  }

  /**
   * Schema service client (ReadSchema, WriteSchema, ReflectSchema, etc.)
   */
  schema(): v1.ZedClientInterface {
    return this.client;
  }

  /**
   * Watch service client (watch for relationship changes)
   */
  watch(): v1.ZedClientInterface {
    return this.client;
  }

  /**
   * Dispose the instance and close the channel.
   */
  close(): void {
    this.client.close();
    spicedb_dispose(this.handle);
  }
}
