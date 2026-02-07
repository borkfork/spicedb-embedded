using System.Runtime.InteropServices;
using System.Text.Json;

namespace Rendil.Spicedb.Embedded;

/// <summary>
/// P/Invoke bindings to the SpiceDB C-shared library (shared/c).
/// Build shared/c first: mise run shared-c-build
/// </summary>
internal static class SpiceDBFFI
{
    private const string LibraryName = "spicedb";

    static SpiceDBFFI()
    {
        NativeLibrary.SetDllImportResolver(typeof(SpiceDBFFI).Assembly, (name, assembly, path) =>
        {
            if (name == LibraryName)
            {
                var libPath = FindLibraryPath();
                if (libPath != null)
                {
                    return NativeLibrary.Load(libPath);
                }
            }
            return IntPtr.Zero;
        });
    }

    [DllImport(LibraryName, EntryPoint = "spicedb_start", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr SpicedbStart();

    [DllImport(LibraryName, EntryPoint = "spicedb_dispose", CallingConvention = CallingConvention.Cdecl)]
    private static extern IntPtr SpicedbDispose(ulong handle);

    [DllImport(LibraryName, EntryPoint = "spicedb_free", CallingConvention = CallingConvention.Cdecl)]
    private static extern void SpicedbFree(IntPtr ptr);

    public static StartResponse Start()
    {
        var ptr = SpicedbStart();
        if (ptr == IntPtr.Zero)
        {
            throw new SpiceDBException("Null response from C library");
        }

        try
        {
            var json = Marshal.PtrToStringUTF8(ptr) ?? throw new SpiceDBException("Invalid UTF-8 from C library");
            var doc = JsonDocument.Parse(json);
            var root = doc.RootElement;

            if (!root.TryGetProperty("success", out var successProp) || !successProp.GetBoolean())
            {
                var err = root.TryGetProperty("error", out var errProp)
                    ? errProp.GetString()
                    : "Unknown error";
                throw new SpiceDBException(err ?? "Unknown error");
            }

            var data = root.GetProperty("data");
            var handle = data.GetProperty("handle").GetUInt64();
            var socketPath = data.GetProperty("socket_path").GetString()
                ?? throw new SpiceDBException("Missing socket_path in response");

            return new StartResponse(handle, socketPath);
        }
        finally
        {
            SpicedbFree(ptr);
        }
    }

    public static void Dispose(ulong handle)
    {
        var ptr = SpicedbDispose(handle);
        if (ptr != IntPtr.Zero)
        {
            try
            {
                var json = Marshal.PtrToStringUTF8(ptr);
                if (!string.IsNullOrEmpty(json))
                {
                    var doc = JsonDocument.Parse(json);
                    if (doc.RootElement.TryGetProperty("success", out var success) && !success.GetBoolean())
                    {
                        if (doc.RootElement.TryGetProperty("error", out var err))
                        {
                            throw new SpiceDBException(err.GetString() ?? "Unknown error");
                        }
                    }
                }
            }
            finally
            {
                SpicedbFree(ptr);
            }
        }
    }

    private static string? FindLibraryPath()
    {
        var explicitPath = Environment.GetEnvironmentVariable("SPICEDB_LIBRARY_PATH");
        if (!string.IsNullOrEmpty(explicitPath))
        {
            return explicitPath;
        }

        var libName = OperatingSystem.IsMacOS() ? "libspicedb.dylib" : "libspicedb.so";

        // Search from CWD and from assembly location (for tests running from bin/Debug/net9.0)
        var searchDirs = new List<string> { Directory.GetCurrentDirectory() };
        var asmDir = Path.GetDirectoryName(typeof(SpiceDBFFI).Assembly.Location);
        if (!string.IsNullOrEmpty(asmDir))
        {
            searchDirs.Add(asmDir);
            // From bin/Debug/net9.0, project root is ../../.. for Tests or ../.. for main
            searchDirs.Add(Path.GetFullPath(Path.Combine(asmDir, "../..")));
            searchDirs.Add(Path.GetFullPath(Path.Combine(asmDir, "../../..")));
            searchDirs.Add(Path.GetFullPath(Path.Combine(asmDir, "../../../..")));
        }

        foreach (var baseDir in searchDirs.Distinct())
        {
            foreach (var rel in new[] { "shared/c", "../shared/c", "csharp/../shared/c" })
            {
                var path = Path.GetFullPath(Path.Combine(baseDir, rel, libName));
                if (File.Exists(path))
                {
                    return path;
                }
            }
        }

        return null;
    }

    public readonly record struct StartResponse(ulong Handle, string SocketPath);
}
