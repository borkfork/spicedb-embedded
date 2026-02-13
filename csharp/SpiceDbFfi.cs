using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Text.RegularExpressions;

namespace Borkfork.SpiceDb.Embedded;

/// <summary>
///     P/Invoke bindings to the SpiceDB C-shared library (shared/c).
///     Build shared/c first: mise run shared-c-build
/// </summary>
internal static class SpiceDbFfi
{
    private const string LibraryName = "spicedb";

    static SpiceDbFfi()
    {
        NativeLibrary.SetDllImportResolver(typeof(SpiceDbFfi).Assembly, (name, _, _) =>
        {
            if (name == LibraryName)
            {
                var libPath = FindLibraryPath();
                if (libPath != null) return NativeLibrary.Load(libPath);
            }

            return IntPtr.Zero;
        });
    }

    [DllImport(LibraryName, EntryPoint = "spicedb_start", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr SpicedbStart(IntPtr optionsJson);

    [DllImport(LibraryName, EntryPoint = "spicedb_dispose", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr SpicedbDispose(ulong handle);

    [DllImport(LibraryName, EntryPoint = "spicedb_free", CallingConvention = CallingConvention.Cdecl)]
    private static extern void SpicedbFree(IntPtr ptr);

    public static StartResponse Start(StartOptions? options = null)
    {
        var optionsPtr = IntPtr.Zero;
        if (options.HasValue)
        {
            var opts = new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower };
            var json = JsonSerializer.Serialize(options.Value, opts);
            var bytes = Encoding.UTF8.GetBytes(json + "\0");
            optionsPtr = Marshal.AllocHGlobal(bytes.Length);
            Marshal.Copy(bytes, 0, optionsPtr, bytes.Length);
        }

        try
        {
            var ptr = SpicedbStart(optionsPtr);
            if (ptr == IntPtr.Zero) throw new SpiceDbException("Null response from C library");

            try
            {
                var json = Marshal.PtrToStringUTF8(ptr) ?? throw new SpiceDbException("Invalid UTF-8 from C library");
                var doc = JsonDocument.Parse(json);
                var root = doc.RootElement;

                if (!root.TryGetProperty("success", out var successProp) || !successProp.GetBoolean())
                {
                    var err = root.TryGetProperty("error", out var errProp)
                        ? errProp.GetString()
                        : "Unknown error";
                    throw new SpiceDbException(err ?? "Unknown error");
                }

                var data = root.GetProperty("data");
                var handle = data.GetProperty("handle").GetUInt64();
                var grpcTransport = data.GetProperty("grpc_transport").GetString()
                                    ?? throw new SpiceDbException("Missing grpc_transport in response");
                var address = data.GetProperty("address").GetString()
                              ?? throw new SpiceDbException("Missing address in response");

                return new StartResponse(handle, grpcTransport, address);
            }
            finally
            {
                SpicedbFree(ptr);
            }
        }
        finally
        {
            if (optionsPtr != IntPtr.Zero) Marshal.FreeHGlobal(optionsPtr);
        }
    }

    public static void Dispose(ulong handle)
    {
        var ptr = SpicedbDispose(handle);
        if (ptr != IntPtr.Zero)
            try
            {
                var json = Marshal.PtrToStringUTF8(ptr);
                if (!string.IsNullOrEmpty(json))
                {
                    var doc = JsonDocument.Parse(json);
                    if (doc.RootElement.TryGetProperty("success", out var success) && !success.GetBoolean())
                        if (doc.RootElement.TryGetProperty("error", out var err))
                            throw new SpiceDbException(err.GetString() ?? "Unknown error");
                }
            }
            finally
            {
                SpicedbFree(ptr);
            }
    }

    private static string? FindLibraryPath()
    {
        var explicitPath = Environment.GetEnvironmentVariable("SPICEDB_LIBRARY_PATH");
        if (!string.IsNullOrEmpty(explicitPath)) return explicitPath;

        var libName = OperatingSystem.IsMacOS()
            ? "libspicedb.dylib"
            : OperatingSystem.IsWindows()
                ? "spicedb.dll"
                : "libspicedb.so";

        // Prefer native lib packaged with the NuGet package (runtimes/<rid>/native/).
        // RuntimeIdentifier can include qualifiers (e.g. win10-x64, osx.14-arm64, linux-musl-x64);
        // we package under fixed RIDs (win-x64, osx-arm64, linux-x64, linux-arm64), so try raw and normalized.
        var asmDir = Path.GetDirectoryName(typeof(SpiceDbFfi).Assembly.Location);
        if (!string.IsNullOrEmpty(asmDir))
        {
            foreach (var rid in GetPackagedRidCandidates())
            {
                var packaged = Path.Combine(asmDir, "runtimes", rid, "native", libName);
                if (File.Exists(packaged)) return packaged;
                var parentRuntimes = Path.Combine(asmDir, "..", "runtimes", rid, "native", libName);
                var parentResolved = Path.GetFullPath(parentRuntimes);
                if (File.Exists(parentResolved)) return parentResolved;
            }
        }

        return null;
    }

    /// <summary>Returns RID candidates to try for runtimes/ (raw first, then normalized to match our packaged names).</summary>
    private static IEnumerable<string> GetPackagedRidCandidates()
    {
        var rid = RuntimeInformation.RuntimeIdentifier;
        if (string.IsNullOrEmpty(rid)) yield break;

        yield return rid;

        // Normalize to match our workflow packaging: win-x64, osx-arm64, linux-x64, linux-arm64
        var normalized = rid;
        normalized = Regex.Replace(normalized, @"^win\d+-", "win-");
        normalized = Regex.Replace(normalized, @"^osx\.\d+-", "osx-");
        normalized = Regex.Replace(normalized, "^linux-musl-", "linux-");
        if (normalized != rid) yield return normalized;
    }

    public readonly record struct StartResponse(ulong Handle, string Transport, string Address);
}

/// <summary>
///     Options for starting an embedded SpiceDB instance.
/// </summary>
// ReSharper disable UnusedMember.Global -- Properties are used by JsonSerializer in SpiceDbFfi.Start
public record struct StartOptions
{
    /// <summary>Datastore: "memory" (default), "postgres", "cockroachdb", "spanner", "mysql".</summary>
    public string? Datastore { get; init; }

    /// <summary>Connection string for remote datastores.</summary>
    public string? DatastoreUri { get; init; }

    /// <summary>gRPC transport: "unix" (default on Unix), "tcp" (default on Windows).</summary>
    public string? GrpcTransport { get; init; }

    /// <summary>Path to Spanner service account JSON (Spanner only).</summary>
    public string? SpannerCredentialsFile { get; init; }

    /// <summary>Spanner emulator host (Spanner only).</summary>
    public string? SpannerEmulatorHost { get; init; }

    /// <summary>Prefix for all tables (MySQL only).</summary>
    [JsonPropertyName("mysql_table_prefix")]
    public string? MySqlTablePrefix { get; init; }

    /// <summary>Enable datastore Prometheus metrics (default: false; disabled allows multiple instances in same process).</summary>
    public bool? MetricsEnabled { get; init; }
}
