using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Text.RegularExpressions;

namespace Borkfork.SpiceDb.Embedded;

/// <summary>
///     P/Invoke bindings to the SpiceDB C-shared library (shared/c).
///     Always uses in-memory transport; unary RPCs via FFI, streaming via proxy.
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

    [DllImport(LibraryName, EntryPoint = "spicedb_free_bytes", CallingConvention = CallingConvention.Cdecl)]
    private static extern void SpicedbFreeBytes(IntPtr ptr);

    [DllImport(LibraryName, EntryPoint = "spicedb_permissions_check_permission", CallingConvention = CallingConvention.Cdecl)]
    private static extern void SpicedbPermissionsCheckPermission(ulong handle, byte[] requestBytes, int requestLen,
        out IntPtr outResponseBytes, out int outResponseLen, out IntPtr outError);

    [DllImport(LibraryName, EntryPoint = "spicedb_schema_write_schema", CallingConvention = CallingConvention.Cdecl)]
    private static extern void SpicedbSchemaWriteSchema(ulong handle, byte[] requestBytes, int requestLen,
        out IntPtr outResponseBytes, out int outResponseLen, out IntPtr outError);

    [DllImport(LibraryName, EntryPoint = "spicedb_permissions_write_relationships", CallingConvention = CallingConvention.Cdecl)]
    private static extern void SpicedbPermissionsWriteRelationships(ulong handle, byte[] requestBytes, int requestLen,
        out IntPtr outResponseBytes, out int outResponseLen, out IntPtr outError);

    [DllImport(LibraryName, EntryPoint = "spicedb_permissions_delete_relationships", CallingConvention = CallingConvention.Cdecl)]
    private static extern void SpicedbPermissionsDeleteRelationships(ulong handle, byte[] requestBytes, int requestLen,
        out IntPtr outResponseBytes, out int outResponseLen, out IntPtr outError);

    [DllImport(LibraryName, EntryPoint = "spicedb_permissions_check_bulk_permissions", CallingConvention = CallingConvention.Cdecl)]
    private static extern void SpicedbPermissionsCheckBulkPermissions(ulong handle, byte[] requestBytes, int requestLen,
        out IntPtr outResponseBytes, out int outResponseLen, out IntPtr outError);

    [DllImport(LibraryName, EntryPoint = "spicedb_permissions_expand_permission_tree", CallingConvention = CallingConvention.Cdecl)]
    private static extern void SpicedbPermissionsExpandPermissionTree(ulong handle, byte[] requestBytes, int requestLen,
        out IntPtr outResponseBytes, out int outResponseLen, out IntPtr outError);

    [DllImport(LibraryName, EntryPoint = "spicedb_schema_read_schema", CallingConvention = CallingConvention.Cdecl)]
    private static extern void SpicedbSchemaReadSchema(ulong handle, byte[] requestBytes, int requestLen,
        out IntPtr outResponseBytes, out int outResponseLen, out IntPtr outError);

    private static byte[] CopyResponseAndFree(IntPtr outResp, int outLen, IntPtr outErr)
    {
        if (outErr != IntPtr.Zero)
        {
            try
            {
                var errMsg = Marshal.PtrToStringUTF8(outErr) ?? "Unknown error";
                throw new SpiceDbException(errMsg);
            }
            finally
            {
                SpicedbFree(outErr);
            }
        }

        if (outLen <= 0 || outResp == IntPtr.Zero) return Array.Empty<byte>();
        var copy = new byte[outLen];
        Marshal.Copy(outResp, copy, 0, outLen);
        SpicedbFreeBytes(outResp);
        return copy;
    }

    internal static byte[] CheckPermission(ulong handle, byte[] requestBytes)
    {
        SpicedbPermissionsCheckPermission(handle, requestBytes, requestBytes.Length, out var r, out var len, out var e);
        return CopyResponseAndFree(r, len, e);
    }

    internal static byte[] WriteSchema(ulong handle, byte[] requestBytes)
    {
        SpicedbSchemaWriteSchema(handle, requestBytes, requestBytes.Length, out var r, out var len, out var e);
        return CopyResponseAndFree(r, len, e);
    }

    internal static byte[] WriteRelationships(ulong handle, byte[] requestBytes)
    {
        SpicedbPermissionsWriteRelationships(handle, requestBytes, requestBytes.Length, out var r, out var len, out var e);
        return CopyResponseAndFree(r, len, e);
    }

    internal static byte[] DeleteRelationships(ulong handle, byte[] requestBytes)
    {
        SpicedbPermissionsDeleteRelationships(handle, requestBytes, requestBytes.Length, out var r, out var len, out var e);
        return CopyResponseAndFree(r, len, e);
    }

    internal static byte[] CheckBulkPermissions(ulong handle, byte[] requestBytes)
    {
        SpicedbPermissionsCheckBulkPermissions(handle, requestBytes, requestBytes.Length, out var r, out var len, out var e);
        return CopyResponseAndFree(r, len, e);
    }

    internal static byte[] ExpandPermissionTree(ulong handle, byte[] requestBytes)
    {
        SpicedbPermissionsExpandPermissionTree(handle, requestBytes, requestBytes.Length, out var r, out var len, out var e);
        return CopyResponseAndFree(r, len, e);
    }

    internal static byte[] ReadSchema(ulong handle, byte[] requestBytes)
    {
        SpicedbSchemaReadSchema(handle, requestBytes, requestBytes.Length, out var r, out var len, out var e);
        return CopyResponseAndFree(r, len, e);
    }

    public static StartResponse Start(StartOptions? options = null)
    {
        var opts = options ?? default;
        var optsJson = new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower };
        var json = JsonSerializer.Serialize(opts, optsJson);
        var bytes = Encoding.UTF8.GetBytes(json + "\0");
        var optionsPtr = Marshal.AllocHGlobal(bytes.Length);
        try
        {
            Marshal.Copy(bytes, 0, optionsPtr, bytes.Length);
            var ptr = SpicedbStart(optionsPtr);
            if (ptr == IntPtr.Zero) throw new SpiceDbException("Null response from C library");

            try
            {
                var jsonResp = Marshal.PtrToStringUTF8(ptr) ?? throw new SpiceDbException("Invalid UTF-8 from C library");
                var doc = JsonDocument.Parse(jsonResp);
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
                var streamingAddress = data.GetProperty("streaming_address").GetString()
                                      ?? throw new SpiceDbException("Missing streaming_address in response");
                var streamingTransport = data.GetProperty("streaming_transport").GetString()
                                         ?? throw new SpiceDbException("Missing streaming_transport in response");
                return new StartResponse(handle, streamingAddress, streamingTransport);
            }
            finally
            {
                SpicedbFree(ptr);
            }
        }
        finally
        {
            Marshal.FreeHGlobal(optionsPtr);
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

    private static IEnumerable<string> GetPackagedRidCandidates()
    {
        var rid = RuntimeInformation.RuntimeIdentifier;
        if (string.IsNullOrEmpty(rid)) yield break;

        yield return rid;

        var normalized = rid;
        normalized = Regex.Replace(normalized, @"^win\d+-", "win-");
        normalized = Regex.Replace(normalized, @"^osx\.\d+-", "osx-");
        normalized = Regex.Replace(normalized, "^linux-musl-", "linux-");
        if (normalized != rid) yield return normalized;
    }

    public readonly record struct StartResponse(ulong Handle, string StreamingAddress, string StreamingTransport);
}

/// <summary>
///     Options for starting an embedded SpiceDB instance (in-memory only).
/// </summary>
// ReSharper disable UnusedMember.Global, UnusedAutoPropertyAccessor.Global -- Properties used by JsonSerializer
public record struct StartOptions
{
    /// <summary>Datastore: "memory" (default), "postgres", "cockroachdb", "spanner", "mysql".</summary>
    public string? Datastore { get; init; }

    /// <summary>Connection string for remote datastores.</summary>
    public string? DatastoreUri { get; init; }

    /// <summary>Path to Spanner service account JSON (Spanner only).</summary>
    public string? SpannerCredentialsFile { get; init; }

    /// <summary>Spanner emulator host (Spanner only).</summary>
    public string? SpannerEmulatorHost { get; init; }

    /// <summary>Prefix for all tables (MySQL only).</summary>
    [JsonPropertyName("mysql_table_prefix")]
    public string? MySqlTablePrefix { get; init; }

    /// <summary>Enable datastore Prometheus metrics (default: false).</summary>
    public bool? MetricsEnabled { get; init; }
}
