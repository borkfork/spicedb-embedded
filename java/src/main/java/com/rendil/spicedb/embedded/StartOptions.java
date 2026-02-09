package com.rendil.spicedb.embedded;

import com.google.gson.annotations.SerializedName;

/**
 * Options for starting an embedded SpiceDB instance.
 *
 * <p>Pass to {@link EmbeddedSpiceDB#create(String, List, StartOptions)} to configure datastore and
 * transport. Use {@code null} for defaults (memory datastore, unix/tcp by platform).
 */
public final class StartOptions {

  /**
   * Datastore: "memory" (default), "postgres", "cockroachdb", "spanner", "mysql". Required for
   * postgres, cockroachdb, spanner, mysql: set {@link #datastoreUri}.
   */
  public String datastore;

  /**
   * Connection string for remote datastores. Required for postgres, cockroachdb, spanner, mysql.
   * E.g. {@code postgres://user:pass@localhost:5432/spicedb}
   */
  @SerializedName("datastore_uri")
  public String datastoreUri;

  /** gRPC transport: "unix" (default on Unix), "tcp" (default on Windows). */
  @SerializedName("grpc_transport")
  public String grpcTransport;

  /**
   * Path to Spanner service account JSON (Spanner only). Omit for Application Default Credentials.
   */
  @SerializedName("spanner_credentials_file")
  public String spannerCredentialsFile;

  /** Spanner emulator host, e.g. "localhost:9010" (Spanner only). */
  @SerializedName("spanner_emulator_host")
  public String spannerEmulatorHost;

  /** Prefix for all tables (MySQL only). Optional, for multi-tenant. */
  @SerializedName("mysql_table_prefix")
  public String mysqlTablePrefix;

  /** Create with defaults. Use setters or builder pattern. */
  public StartOptions() {}

  /** Builder for fluent option construction. */
  public static Builder builder() {
    return new Builder();
  }

  public static final class Builder {
    private final StartOptions opts = new StartOptions();

    public Builder datastore(String datastore) {
      opts.datastore = datastore;
      return this;
    }

    public Builder datastoreUri(String uri) {
      opts.datastoreUri = uri;
      return this;
    }

    public Builder grpcTransport(String transport) {
      opts.grpcTransport = transport;
      return this;
    }

    public Builder spannerCredentialsFile(String path) {
      opts.spannerCredentialsFile = path;
      return this;
    }

    public Builder spannerEmulatorHost(String host) {
      opts.spannerEmulatorHost = host;
      return this;
    }

    public Builder mysqlTablePrefix(String prefix) {
      opts.mysqlTablePrefix = prefix;
      return this;
    }

    public StartOptions build() {
      return opts;
    }
  }
}
