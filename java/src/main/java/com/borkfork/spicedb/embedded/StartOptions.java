package com.borkfork.spicedb.embedded;

import com.google.gson.annotations.SerializedName;

/**
 * Options for starting an embedded SpiceDB instance.
 *
 * <p>Pass to {@link EmbeddedSpiceDB#create(String, List, StartOptions)} to configure datastore. Use
 * {@code null} for defaults (memory datastore).
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

  /**
   * Primary switch for all metrics and tracing (default: false). When false, all other
   * observability options are ignored.
   */
  @SerializedName("metrics_enabled")
  public Boolean metricsEnabled;

  /**
   * Enable datastore Prometheus metrics (default: true when metricsEnabled=true). Only takes effect
   * when metricsEnabled=true.
   */
  @SerializedName("datastore_metrics_enabled")
  public Boolean datastoreMetricsEnabled;

  /**
   * Enable cache Prometheus metrics for dispatch/namespace/cluster caches (default: true when
   * metricsEnabled=true). Only takes effect when metricsEnabled=true.
   */
  @SerializedName("cache_metrics_enabled")
  public Boolean cacheMetricsEnabled;

  /**
   * OTLP gRPC endpoint for OpenTelemetry traces, e.g. {@code "localhost:4317"} (insecure). Only
   * used when metricsEnabled=true.
   */
  @SerializedName("otlp_endpoint")
  public String otlpEndpoint;

  /**
   * If set, starts a Prometheus HTTP server on this port at {@code /metrics}. Only used when
   * metricsEnabled=true.
   */
  @SerializedName("metrics_port")
  public Integer metricsPort;

  /**
   * Host/IP the Prometheus HTTP server binds to (default: {@code "0.0.0.0"}). Only used when
   * metricsEnabled=true and metricsPort is set.
   */
  @SerializedName("metrics_host")
  public String metricsHost;

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

    public Builder metricsEnabled(boolean enabled) {
      opts.metricsEnabled = enabled;
      return this;
    }

    public Builder datastoreMetricsEnabled(boolean enabled) {
      opts.datastoreMetricsEnabled = enabled;
      return this;
    }

    public Builder cacheMetricsEnabled(boolean enabled) {
      opts.cacheMetricsEnabled = enabled;
      return this;
    }

    public Builder otlpEndpoint(String endpoint) {
      opts.otlpEndpoint = endpoint;
      return this;
    }

    public Builder metricsPort(int port) {
      opts.metricsPort = port;
      return this;
    }

    public Builder metricsHost(String host) {
      opts.metricsHost = host;
      return this;
    }

    public StartOptions build() {
      return opts;
    }
  }
}
