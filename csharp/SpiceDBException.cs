namespace Borkfork.SpiceDb.Embedded;

/// <summary>
///     Thrown when SpiceDB operations fail.
/// </summary>
public sealed class SpiceDbException : Exception
{
    public SpiceDbException(string message) : base(message)
    {
    }

    public SpiceDbException(string message, Exception inner) : base(message, inner)
    {
    }
}
