package com.borkfork.spicedb.embedded;

import com.authzed.api.v1.*;

/** Schema client (ReadSchema, WriteSchema) via FFI. */
public final class EmbeddedSchemaStub {

  private final long handle;

  EmbeddedSchemaStub(long handle) {
    this.handle = handle;
  }

  public ReadSchemaResponse readSchema(ReadSchemaRequest request) {
    byte[] raw = SpiceDBFfi.readSchema(handle, request.toByteArray());
    if (raw.length == 0) {
      return ReadSchemaResponse.getDefaultInstance();
    }
    try {
      return ReadSchemaResponse.parseFrom(raw);
    } catch (Exception e) {
      throw new SpiceDBException("Failed to parse ReadSchemaResponse", e);
    }
  }

  public WriteSchemaResponse writeSchema(WriteSchemaRequest request) {
    byte[] raw = SpiceDBFfi.writeSchema(handle, request.toByteArray());
    if (raw.length == 0) {
      return WriteSchemaResponse.getDefaultInstance();
    }
    try {
      return WriteSchemaResponse.parseFrom(raw);
    } catch (Exception e) {
      throw new SpiceDBException("Failed to parse WriteSchemaResponse", e);
    }
  }
}
