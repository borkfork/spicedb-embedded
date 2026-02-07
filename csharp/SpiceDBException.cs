namespace Rendil.Spicedb.Embedded;

/// <summary>
/// Thrown when SpiceDB operations fail.
/// </summary>
public sealed class SpiceDBException : Exception
{
    public SpiceDBException(string message) : base(message) { }

    public SpiceDBException(string message, Exception inner) : base(message, inner) { }
}
